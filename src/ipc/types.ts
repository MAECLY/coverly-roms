/** Wire types: mirror of src-tauri/src/library/{view,identify}.rs. */

import type {
  DupGroupView,
  InLibraryReason,
  OrganizeDedupeMode,
  OrganizeSetAsideAction,
} from './dedupeTypes';
import type { ErrorCode } from './errors';

export * from './dedupeTypes';

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
  /** ROM-like files directly in the root or the ROMs folder, outside any
   * system folder (what "Organise loose files" sorts). */
  loose_files: number;
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
  /** The sources that refused the gather and why (Rust leaves the field
   * out when none did): notices link them to their integration card. */
  troubles?: SourceTrouble[];
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
  | { why: 'embedded-first' }
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
  /** What the job did: fill art or organise a pile of loose ROMs. */
  kind: JobKind;
  /** False for a journal written by a newer Coverly. */
  undoable: boolean;
}

export type JobKind = 'fill' | 'organize' | 'dedupe';

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
/** Organise transfer mode (`auto` = move on the same volume, copy otherwise). */
export type OrganizeModeSetting = 'auto' | 'move' | 'copy';
/** Whether Organise may run an external archive tool (unrar, 7-Zip, bsdtar). */
export type ExternalToolsSetting = 'auto' | 'off';
/** Organised files keep their name or take the catalogue name. */
export type OrganizeNameMode = 'keep' | 'catalogue';

export interface OrganizeSettings {
  default_layout: string;
  nest_in_roms_root: boolean;
  mode: OrganizeModeSetting;
  names: OrganizeNameMode;
  download_dats: boolean;
  auto_threshold: number;
  review_threshold: number;
  companion_exts: string[];
  /** Profile id → system id → folder name. */
  folder_names: Record<string, Record<string, string>>;
  /** Profile id → arcade system order. */
  arcade_order: Record<string, string[]>;
  /** Profile id → game folders on or off (absent = the profile's own
   * value; game folders spec D1). Not sent while empty. */
  game_folders?: Record<string, boolean>;
  recent_seconds: number;
  keep_organize_journals_days: number;
  external_tools: ExternalToolsSetting;
}
export type MatchMethod = 'hash-then-name' | 'hash-only' | 'name-only';
export type SourceId =
  'libretro' | 'steamgriddb' | 'screenscraper' | 'embedded';

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
  organize: OrganizeSettings;
  /** Optional, downloaded name catalogues (adjustments D2). */
  catalogs: CatalogSettings;
}

/** Mirror of `coverly_core::settings::SwitchCatalogSettings`: the optional
 * `blawar/titledb` name catalogue, off by default. `icons` and `fallback`
 * count only while `enabled` (the core turns them off with it). */
export interface SwitchCatalogSettings {
  enabled: boolean;
  icons: boolean;
  /** 7–90. */
  refresh_days: number;
  fallback: boolean;
  /** HTTPS base URL, no query or credentials. */
  fallback_url: string;
}

export interface CatalogSettings {
  switch: SwitchCatalogSettings;
}

export type SettingsGroup = Exclude<keyof Settings, 'version'>;

/** A merge patch: maps merge key by key, so removing a per-profile max
 * size or game folders override takes an explicit `null` for that key. */
export type SettingsPatch = {
  [G in SettingsGroup]?: G extends 'art'
    ? Partial<Omit<Settings['art'], 'max_size'>> & {
        max_size?: Record<string, MaxSize | null>;
      }
    : G extends 'catalogs'
      ? { switch?: Partial<SwitchCatalogSettings> | null }
      : G extends 'organize'
        ? Partial<Omit<Settings['organize'], 'game_folders'>> & {
            game_folders?: Record<string, boolean | null>;
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
  /** The external archive tools found on this machine, in preference
   * order, whatever `organize.external_tools` says. */
  external_tools_found: FoundTool[];
}

/** One external archive tool found on this machine. */
export interface FoundTool {
  tool: ArchiveTool;
  /** From the tool's banner (`24.08`, `7.01`). */
  version: string;
  /** Absolute path of the executable. */
  path: string;
}

export interface SettingsUpdate {
  settings: Settings;
  errors: FieldError[];
  reopen: boolean;
}

export type LinkId =
  | 'repo'
  | 'license'
  | 'notices'
  | 'trademarks'
  | 'releases'
  | 'issues'
  /** The roadmap the «Próximamente» list of Integraciones links to. */
  | 'roadmap';

/** Which link of a credit entry `open_credit` opens (adjustments D27). */
export type CreditLink = 'site' | 'license';

/** Mirror of `coverly_core::catalog::CatalogStatus`: the optional Switch
 * name catalogue as the Sources group shows it (spec §7, §10). `updated`
 * is an ISO date; `fallback` = the United States list instead of the UI
 * language's. */
export type CatalogStatus =
  | { state: 'off' }
  | { state: 'pending' }
  | { state: 'downloading'; done: number; total: number | null }
  | { state: 'ready'; count: number; updated: string; fallback: boolean }
  | { state: 'failed-old'; updated: string }
  | { state: 'failed-none' }
  | { state: 'rejected' }
  | { state: 'remove-failed' };

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

/** A card's state (integrations spec D6), as
 * `coverly_core::integrations::State`. Precedence, strongest first: `off` >
 * `unavailable` > `not-configured` > `quota` > `error` > `connected` >
 * `saved`. */
export type IntegrationState =
  | 'off'
  | 'unavailable'
  | 'not-configured'
  | 'saved'
  | 'connected'
  | 'error'
  | 'quota';

/** Why the keychain could not be read (D16), without its message. */
export type KeychainKind = 'unavailable' | 'locked' | 'denied' | 'failed';

/** The kind of input a credential field (or a planned service's need) is. */
export type FieldInput =
  'user' | 'password' | 'api-key' | 'client-id' | 'secret';

/** Why a card is in `error` or `not-configured`. */
export type IntegrationReason =
  | { kind: 'check'; detail: KeyCheck }
  | { kind: 'keychain'; detail: KeychainKind }
  | { kind: 'missing'; detail: FieldInput };

/** ScreenScraper's quota as its account answer gives it. */
export interface SsQuota {
  requests_today: number;
  max_requests_per_day: number;
  max_threads: number;
}

/** One card of the «Integraciones» tab (Rust `IntegrationView`). Values
 * never come back: each field says only whether it is stored. Optional
 * keys are left out by Rust when absent. */
export interface IntegrationView {
  id: string;
  enabled: boolean;
  /** This build can use it (ScreenScraper needs developer credentials). */
  built: boolean;
  fields: { key: CredentialKey; set: boolean }[];
  state: IntegrationState;
  reason?: IntegrationReason;
  /** When the last test or job said what `state` shows (unix seconds). */
  checked_at?: number;
  quota?: SsQuota;
  /** When `quota` was read or, without numbers, when the quota was seen
   * used up (unix seconds). */
  quota_at?: number;
}

/** What «Probar conexión» found. */
export interface IntegrationTestResult {
  verdict: KeyCheck;
  quota?: SsQuota;
}

/** What a notice on another screen says about an integration (D28). */
export type IntegrationProblem = 'missing' | 'rejected' | 'quota' | 'off';
export interface IntegrationNotice {
  integration: string;
  problem: IntegrationProblem;
}

/** A source that refused a run, and why (Rust `SourceTrouble`). */
export interface SourceTrouble {
  source: SourceId;
  kind: 'missing-key' | 'auth' | 'exhausted';
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
  /** How organise names system folders; absent = `first-alias`. */
  system_folder?: FolderNaming;
  /** Per-system folder names (system id → folder); absent = none. */
  folder_names?: Record<string, string>;
  /** Systems kept in another system's folder (system id → host id, e.g.
   * `ngpc → ngp` on Anbernic cards); absent = none. */
  host_folders?: Record<string, string>;
  /** Where disc games are extracted; absent = flat without `.m3u`. */
  disc_layout?: DiscLayout;
  /** Containers whose cartridge ROMs move as they are; absent = zip, 7z. */
  accepts_containers?: ContainerKind[];
  /** Per-system replacements of `accepts_containers`; absent = `psp: []`
   * and `ps5: [zip, 7z, rar]`. */
  container_overrides?: Record<string, ContainerKind[]>;
  /** Suffix a folder game of a system must end in (system id → suffix,
   * `ps3 → .ps3` on ES-DE and Batocera); absent = none. */
  folder_game_suffix?: Record<string, string>;
  /** Media of a folder game are named after its whole folder name, suffix
   * included (ES-DE); absent = the suffix is dropped. */
  folder_stem_keeps_suffix?: boolean;
  /** Folder, relative to a system's folder, where the frontend reads DLC
   * (system id → template whose only token is `{title_id}`, `ps4 →
   * DLC/{title_id}` on Batocera); absent = DLC sits beside its game. */
  dlc_folder?: Record<string, string>;
  /** Organise puts a modern game's base, updates and DLC in one folder
   * (game folders spec D1); absent = off. */
  game_folders?: boolean;
  /** The emulator Organizar lays each system out for by default (system id
   * → emulator id, `ps4 → shadps4` on ES-DE; mods spec D16); absent =
   * none. */
  emulators?: Record<string, string>;
}

/** `profiles::DiscLayout`. */
export interface DiscLayout {
  kind: 'flat' | 'game-folder';
  m3u_for_multi: boolean;
}

/** `archive::ContainerKind`, as profile files name it. */
export type ContainerKind =
  | 'zip'
  | '7z'
  | 'rar'
  | 'tar'
  | 'tar.gz'
  | 'tar.bz2'
  | 'tar.xz'
  | 'tar.zst'
  | 'tar.lz4'
  | 'tar.lzma'
  | 'gz'
  | 'bz2'
  | 'xz'
  | 'zst'
  | 'lz4'
  | 'lzma';

/** `profiles::FolderNaming`: `GBA`/`SFC` card names or system ids. */
export type FolderNaming = 'first-alias' | 'id';

/** `systems::Family`: the kind of machine, which groups the pickers. */
export type SystemFamily =
  'console' | 'handheld' | 'computer' | 'arcade' | 'engine';

/** One system Coverly knows (`list_systems`), sorted by name. */
export interface SystemInfo {
  id: string;
  name: string;
  family: SystemFamily;
  /** No public emulator runs it (systems batch 3 D22). */
  no_emulator: boolean;
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

/* Organising a loose pile of ROMs — mirror of coverly_core::organize
 * (plan.rs, apply.rs), coverly_core::identify::evidence and
 * src-tauri/src/organize/view.rs. */

/** How files reach their system folder. */
export type TransferMode = 'move' | 'copy';
/** Organised files keep their name or take the catalogue (DAT) name. */
export type NameMode = OrganizeNameMode;
/** The rung of the identification ladder a clue comes from. */
export type IdLevel =
  'hash' | 'header' | 'extension' | 'name' | 'online' | 'user';

/** One clue: `detail` is data for the UI (a CRC, `CGB 0xC0`, `.gb`), never prose. */
export interface Evidence {
  kind: IdLevel;
  system: string;
  weight: number;
  detail: string;
}

/** Why an archive entry's stored path is never extracted. */
export type UnsafeWhy =
  | 'absolute'
  | 'dot-dot'
  | 'device'
  | 'illegal'
  | 'encoding'
  | 'link'
  | 'too-long';

/** What a file says it is not (Rust `NotARom`). */
export type NotARom =
  | 'web-page'
  | 'text'
  | 'patch'
  | 'pc-game-data'
  /** A macOS installer package named `.pkg`. */
  | 'installer'
  /** An Autodesk 3D Studio model named `.3ds`. */
  | 'model3d'
  /** A console's own system file or folder, never a game. */
  | 'console-system'
  /** An emulator's shader or pipeline cache, recognised by name. */
  | 'emulator-cache'
  /** An emulator's per-game settings file, recognised by name. */
  | 'emulator-config';

/** Why a file stays where it is. */
export type LeftReason =
  | { kind: 'partial-download' }
  | { kind: 'recently-modified' }
  | { kind: 'cloud-only' }
  | { kind: 'not-a-rom'; what: NotARom }
  | { kind: 'bios' }
  | { kind: 'damaged-archive' }
  /** `need` names the tool that would open it (absent in older plans). */
  | {
      kind: 'unsupported-archive';
      format?: string | null;
      need?: ArchiveTool | null;
    }
  | { kind: 'encrypted-archive' }
  /** Decoding needs more memory than the cap. */
  | { kind: 'archive-needs-memory'; mib: number }
  /** Too many entries or declared bytes to open safely (not damage). */
  | { kind: 'archive-too-big'; entries: number | null; bytes: number | null }
  | { kind: 'multi-volume-archive'; parts: number }
  | { kind: 'unsafe-archive-entry'; name: string; why: UnsafeWhy }
  /** A set member's name the target cannot store unchanged. */
  | { kind: 'set-unsafe-name'; name: string }
  | { kind: 'path-too-long' }
  /** The only payload is another archive. */
  | { kind: 'nested-archive' }
  /** An arcade set in a container other than zip. */
  | { kind: 'repack-arcade' }
  /** The FAT32 folder has no directory slots left. */
  | { kind: 'folder-full'; folder: string }
  /** The target folder is a symlink or junction. */
  | { kind: 'unsafe-target' }
  | { kind: 'multi-game-archive'; count: number }
  | { kind: 'unsupported-system'; name: string }
  | { kind: 'unknown' }
  | { kind: 'duplicate'; of: string }
  | { kind: 'already-present'; at: string }
  | { kind: 'already-sorted'; folder: string }
  /** A game that is a folder (Doom WAD, Solarus, Lutro, ScummVM): left as
   * one item; `folder` is the system's folder under the target layout
   * (spec D15). */
  | { kind: 'folder-game'; system: string; folder: string }
  /** The game is already in the library (duplicates step). */
  | InLibraryReason
  | { kind: 'symlink' }
  | { kind: 'set-incomplete'; missing: string[] }
  | { kind: 'set-outside-folder' }
  | { kind: 'shared-track' }
  | { kind: 'ambiguous-companion' }
  | { kind: 'too-big-for-fat' }
  | { kind: 'unreadable' }
  | { kind: 'collision-set' }
  /** A console key or licence file: never read, never moved on its own. */
  | { kind: 'key-material' }
  /** A licence that goes with its game as a companion (`of`: the game's
   * source path); never read, moved only by a same-volume rename. */
  | { kind: 'licence-of'; of: string }
  /** Inside an emulator's own data tree; `folder` is its root (`ux0`). */
  | { kind: 'emulator-data'; folder: string }
  /** A mods or patch folder beside its base that cannot move alone; `of`
   * is the base's title (its folder name when the base is not an item). */
  | { kind: 'game-overlay'; of: string }
  /** A piece of a game that cannot play alone (an ExeFS file, a loose NCA,
   * a library, title metadata, a module or a backup); `title_id` is the
   * game's when the piece or its path says. */
  | {
      kind: 'game-part';
      part: PartKind;
      system: string;
      title_id: string | null;
    }
  /** A huge item the user said «No» to: it stays where it is. */
  | { kind: 'not-confirmed' }
  /** A file inside a fixed folder of a game folder (`of`: the game
   * folder's name): it belongs to its game and stays for now. */
  | { kind: 'game-folder-content'; folder: FixedFolder; of: string }
  /** An addon whose game is not known: never moved, never deleted (mods
   * spec D7). `addon` is its kind (`kind` is the tag). */
  | {
      kind: 'addon-unlinked';
      addon: AddonKind;
      format: AddonFormat;
      why: UnlinkedWhy;
    }
  /** Another file of that name is already in its fixed folder (`at`,
   * relative to the target root): never renamed to ` (2)`. */
  | { kind: 'addon-name-taken'; at: string }
  /** A linked addon the layout has no place for. */
  | { kind: 'addon-no-place'; addon: AddonKind }
  /** Goes in the emulator's own data folder (`path` relative to it),
   * which the run did not name; `emulator` is the product name. */
  | { kind: 'addon-for-emulator'; emulator: string; path: string }
  /** The chosen emulator does not use the item like this. */
  | { kind: 'not-supported'; emulator: string; why: NotSupportedWhy }
  /** A patch the emulator would apply when playing, renamed for it only
   * when the user chooses that emulator. */
  | { kind: 'patch-not-applied'; emulator: string };

/** A fixed folder inside a game folder (Rust `FixedFolder`). Its name on
 * disk is English (`Updates`, `DLC`, `Mods`, `Cheats`, `Extras`,
 * `Saves`); the UI translates it. */
export type FixedFolder =
  'updates' | 'dlc' | 'mods' | 'cheats' | 'extras' | 'saves';

/** What an item is to its game (Rust `GameRole`). */
export type GameRole = 'base' | 'update' | 'dlc' | 'demo' | 'mod' | 'cheat';

/** What an addon is to its game (Rust `AddonKind`). */
export type AddonKind = 'patch' | 'cheat' | 'mod';

/** The format of an addon (Rust `AddonFormat`). */
export type AddonFormat =
  | 'bps'
  | 'ups'
  | 'ips'
  | 'xdelta'
  | 'ppf'
  | 'aps'
  | 'retro-arch-cht'
  | 'fceux-cht'
  | 'duck-station-cht'
  | 'pnach'
  | 'dolphin-ini'
  | 'ppsspp-ini'
  | 'ctr-cheats'
  | 'switch-cheats'
  | 'pchtxt'
  | 'rpcs3-patch'
  | 'shadps4-cheat'
  | 'shadps4-patch'
  | 'xenia-patch'
  | 'vita3k-patch'
  | 'melon-ds-cheats'
  | 'mgba-cheats'
  | 'desmume-cheats'
  | 'code-text'
  | 'texture-pack'
  | 'dolphin-graphic-mod'
  | 'riivolution'
  | 'layered-fs'
  | 'switch-mod'
  | 'exefs-patches'
  | 'cemu-graphic-pack'
  | 'vita-repatch'
  | 'mesen-hd-pack'
  | 'msu1'
  | 'xenia-plugin'
  | 'n64-hires-textures'
  | 'shadps4-mods';

/** How an addon was linked to its game (Rust `LinkVia`). */
export type LinkVia =
  'source-crc' | 'dat' | 'name' | 'readme' | 'key' | 'folder';

/** Whether a patch fits the ROM as it is on disk (Rust `HeaderFit`). */
export type HeaderFit = 'matches' | 'needs-headerless' | 'needs-headered';

/** Why an addon stays where it is (Rust `UnlinkedWhy`). */
export type UnlinkedWhy =
  | { kind: 'no-match' }
  | { kind: 'several'; count: number }
  | { kind: 'many'; count: number }
  | { kind: 'no-key' }
  | { kind: 'budget' }
  | { kind: 'partial' };

/** An addon linked to its game (Rust `AddonLink`). */
export interface AddonLink {
  kind: AddonKind;
  format: AddonFormat;
  /** «SLUS-00594», «0004000000055D00», «CRC32 1A2B3C4D»: shown in mono. */
  key_text: string;
  via: LinkVia;
  header?: HeaderFit;
}

/** An emulator a translator knows (Rust `EmulatorId`). */
export type EmulatorId =
  | 'shadps4'
  | 'rpcs3'
  | 'azahar'
  | 'cemu'
  | 'vita3k'
  | 'ryujinx'
  | 'yuzu-family'
  | 'retroarch'
  | 'duckstation'
  | 'pcsx2'
  | 'dolphin'
  | 'ppsspp'
  | 'xenia'
  | 'flycast'
  | 'mesen';

/** The emulator a run chose for one system, or `none` for the library
 * layout (Rust `EmulatorChoice`). */
export type EmulatorChoice = EmulatorId | 'none';

/** Why an emulator does not use an item as the library keeps it. */
export type NotSupportedWhy =
  | 'rpcs3-mods'
  | 'vita-repatch'
  | 'several-patches'
  | 'older-update'
  | 'config-exists';

/** What a translator says about one placement instead of doing it. */
export type TranslateNote =
  | { kind: 'install-in-emulator'; emulator: EmulatorId }
  | { kind: 'extract-first' }
  | { kind: 'not-supported'; emulator: EmulatorId; why: NotSupportedWhy }
  | { kind: 'header-mismatch'; needs: HeaderFit }
  | { kind: 'point-emulator-here'; emulator: EmulatorId; folder: string }
  | { kind: 'patch-not-applied'; emulator: EmulatorId };

/** Where a placement goes: under the device's system folder, under the
 * emulator's own data folder, or nowhere (it stays in the library). */
export type PlacementTo =
  | { kind: 'system'; path: string }
  | { kind: 'emulator-data'; path: string }
  | { kind: 'library' };

/** One placement of a translated game (`from`: relative to the master
 * system folder). */
export interface PlacementView {
  from: string;
  to: PlacementTo;
  note?: TranslateNote;
}

/** A game of the plan laid out for one emulator, for the preview. */
export interface TranslatedGameView {
  system: string;
  /** The base item's id, when the base is an item. */
  base?: number;
  emulator: EmulatorId;
  placements: PlacementView[];
}

/** How an item belongs to a game folder (Rust `GameLink`); optional
 * fields are left out when absent or false. */
export interface GameLink {
  role: GameRole;
  /** Display form, never the raw header value. */
  version?: string;
  region?: Region;
  /** The base item's id in this plan. */
  of?: number;
  /** The game folder, relative to the target root. */
  folder?: string;
  /** The game folder was already on disk. */
  folder_exists?: boolean;
  /** A demo's base title, linked by name only. */
  demo_of?: string;
  /** Highest version present for this base. */
  newest_here?: boolean;
  /** What a `mod` or `cheat` row is and how it was linked. */
  addon?: AddonLink;
}

/** A piece of a game that cannot play alone (Rust `PartKind`). */
export type PartKind =
  'exe-fs' | 'nca' | 'library' | 'title-meta' | 'module' | 'backup';

/** What a console title is (Rust `TitleKind`). */
export type TitleKind =
  'base' | 'update' | 'dlc' | 'demo' | 'homebrew' | 'system' | 'unknown';

/** Where a console title's name came from (Rust `TitleSource`). */
export type TitleSource = 'header' | 'titledb' | 'nlib' | 'file-name';

/** Something worth telling about an item that still moves. */
export type ItemNote =
  | { kind: 'multi-rom-zip'; count: number }
  | { kind: 'no-folder-on-device'; folder: string }
  /** `system`: the extension's owner, whose header is missing or lost to
   * another system's (spec D24); absent in plans made before it. */
  | { kind: 'extension-contradicted'; ext: string; system?: string }
  | { kind: 'catalogue-missing'; system: string }
  | { kind: 'encrypted' }
  /** The magic says another container than the extension (`actual`: `7z`). */
  | { kind: 'container-mismatch'; ext: string; actual: string }
  /** A `.gdi` and a `.cue` for the same tracks: both are extracted. */
  | { kind: 'twin-sheets'; kept: string[] }
  /** The format stores no CRC: only the size is checked. */
  | { kind: 'no-stored-checksum' }
  /** The decode budget ran out during the dry run. */
  | { kind: 'archive-not-fully-read' }
  /** Move mode keeps the archive: it holds files that are not extracted. */
  | { kind: 'archive-kept-leftovers'; count: number; examples: string[] }
  /** The sheet sits beyond the plan-time budget: checked on apply. */
  | { kind: 'sheet-not-read-yet' }
  /** Several ROMs of one system are extracted from one archive. */
  | { kind: 'multi-rom-extract'; count: number }
  /** An update of `of` (its title, or its title id without the base). */
  | { kind: 'update'; of: string }
  /** Downloadable content of `of`. */
  | { kind: 'dlc'; of: string }
  /** The content is encrypted; `emulator` (a fixed name) only opens
   * decrypted copies. */
  | { kind: 'encrypted-content'; emulator: string }
  /** A package the emulator installs (PS3/Vita PKG). */
  | { kind: 'install-first'; emulator: string }
  /** The folder is renamed to `name` so `layout` shows it. */
  | { kind: 'folder-suffix'; name: string; layout: string }
  /** A PS4 PKG: catalogued, has to be extracted to play. */
  | { kind: 'extract-first' }
  /** A PS5 store PKG, `.exfat`, `.ffpfs`, `.ffpfsc` or an XVC. */
  | { kind: 'no-emulator' }
  /** Enhanced for PS5 Pro (`attribute3` bit 22). */
  | { kind: 'pro-enhanced' }
  /** Goes to the DLC folder where `layout` looks for downloadable content. */
  | { kind: 'dlc-folder'; layout: string }
  /** Homebrew: goes to the system folder like any ROM. */
  | { kind: 'homebrew' }
  /** A Vita game kept in a 7z: Vita3K only installs .zip, .vpk or .vci. */
  | { kind: 'vita7z' }
  /** Disc `n` of a multi-disc set is not in the pile. */
  | { kind: 'missing-disc'; n: number }
  /** The title cannot be read without decrypting it («Cifrado»). */
  | { kind: 'encrypted-name' }
  /** The archive holds the folders of `count` games: review. */
  | { kind: 'archive-games'; count: number }
  /** PS4 and Vita share the structure; param.sfo was not read: review. */
  | { kind: 'ps4-or-vita' }
  /** Two names in the archive differ only in case: review. */
  | { kind: 'archive-case-collision' }
  /** Loose discs whose names alone say they are one game: review. */
  | { kind: 'disc-set' }
  /** A demo of `of`, linked by name only: its own game. */
  | { kind: 'demo-of'; of: string }
  /** An update or DLC whose game is not in the plan or the library. */
  | { kind: 'base-missing' }
  /** A mod, patch or cheats of `of` (its title). */
  | { kind: 'mod-of'; of: string }
  | { kind: 'patch-of'; of: string }
  | { kind: 'cheat-of'; of: string }
  /** The patch expects the ROM with or without its header. */
  | { kind: 'header-mismatch'; needs: HeaderFit }
  /** The emulator (product name) has to be pointed at `folder`. */
  | { kind: 'point-emulator-here'; emulator: string; folder: string };

/** Why an item's target name differs from its source name. */
export type RenameReason =
  | { kind: 'collision'; name: string }
  | { kind: 'safe-name'; from: string }
  | { kind: 'path-length' }
  | { kind: 'catalogue'; from: string }
  /** Renamed to its game folder's name. */
  | { kind: 'game-folder'; from: string };

/** Something about the whole plan. */
export type PlanWarning =
  | { kind: 'catalogue-missing'; system: string }
  | { kind: 'target-layout-mismatch'; profile: string }
  | { kind: 'move-unavailable' }
  | { kind: 'sorted-only' }
  /** The target lacks room: cluster-rounded bytes needed vs free for this user. */
  | { kind: 'no-space'; needed: number; free: number }
  /** The items that move by default write at least 10 GB to the target. */
  | { kind: 'large-transfer'; bytes: number; items: number };

/** A save, patch or note that follows its ROM. */
export interface Companion {
  /** Relative to the source root. */
  source: string;
  /** Relative to the target root. */
  target: string;
  /** A licence recognised by its name only (never opened); Rust leaves
   * it out when false. */
  licence?: boolean;
}

/** Size and mtime of a source when planned (apply re-validates them). */
export interface SourceFp {
  size: number;
  mtime_ns: number;
}

/** A game that is a folder moves whole; its `size` is the sum of its files. */
export type OrganizeItemKind = 'file' | 'archive' | 'set' | 'folder';

/** Where a console title id was read (Rust `IdSource`). */
export type IdSource =
  | 'param-sfo'
  | 'meta-xml'
  | 'app-xml'
  | 'default-xbe'
  | 'default-xex'
  | 'cnmt-xml'
  | 'ticket-name'
  | 'file-name'
  | 'header'
  | 'param-json'
  | 'microsoft-game-config'
  | 'appx-manifest'
  | 'nca-catalog'
  | 'tmd'
  | 'npdm'
  | 'xbe'
  | 'xex'
  | 'self-header'
  | 'pbp';

/** A console title id and where it came from. */
export interface TitleIdRef {
  id: string;
  source: IdSource;
}

export type OrganizeAction =
  | { action: 'auto' }
  | { action: 'review' }
  | { action: 'left'; reason: LeftReason }
  | OrganizeSetAsideAction;

/** An external archive program (unrar, 7-Zip, bsdtar; names are not translated). */
export type ArchiveTool = 'unrar' | 'seven-zip' | 'bsdtar';

/** One archive entry an extract item writes. */
export interface ExtractEntry {
  /** Position in the archive's own order. */
  index: number;
  /** The validated entry path, `/`-separated. */
  name: string;
  size: number;
  crc32: number | null;
  /** Relative to the target root. */
  target: string;
}

/** What happens to the archive once every entry is verified. */
export type ArchiveAfter = 'keep' | 'trash';

/** Who decodes the archive on apply. */
export type ExtractSource =
  { kind: 'builtin' } | { kind: 'tool'; tool: ArchiveTool; version: string };

/** Whether a disc set's sheet was parsed at plan time. */
export type SheetCheck = 'read' | 'deferred';

/** How an item reaches its target. */
export type Transfer =
  | { kind: 'as-is' }
  | {
      kind: 'extract';
      entries: ExtractEntry[];
      bytes: number;
      archive_after: ArchiveAfter;
      source: ExtractSource;
      sheet_check: SheetCheck;
      /** A generated playlist, relative to the target root. */
      m3u: string | null;
      /** A folder game extracted out of an archive (adjustments D23):
       * always asked on its own before apply. Absent when false. */
      folder_game?: boolean;
    };

/** The ISO 9660 descriptor and boot-file hash of a disc. */
export interface VolumeFingerprint {
  space_size: number;
  created: number[];
  modified: number[];
  boot_sha1: string;
}

/** A disc's serials and fingerprint. */
export interface DiscId {
  /** Normalised serials (`SLUS-00862`), in disc order. */
  serials: string[];
  disc_no: number | null;
  volume: VolumeFingerprint | null;
  source:
    | 'system-cnf'
    | 'param-sfo'
    | 'ip-bin'
    | 'umd-data'
    | 'chd'
    | 'm3u-position'
    | 'signature';
}

/** One game of the plan: a file, an archive or a disc set. */
export type OrganizeItem = OrganizeAction & {
  id: number;
  /** Relative to the source root; the primary file first. */
  sources: string[];
  kind: OrganizeItemKind;
  size: number;
  fps: SourceFp[];
  /** Relative to the target root (the primary file). */
  target: string | null;
  /** The other members of a set, in the order of `sources[1..]`. */
  set_targets: string[];
  system_id: string | null;
  /** The system folder name actually used. */
  folder: string | null;
  confidence: number;
  method: IdLevel | null;
  why: Evidence[];
  /** Other systems with their weight, best first. */
  alternatives: [string, number][];
  rename: RenameReason | null;
  notes: ItemNote[];
  companions: Companion[];
  catalogue_name: string | null;
  /** As-is (move or copy the file) or extract. */
  transfer: Transfer;
  disc: DiscId | null;
  /** The console title id and its source (Rust leaves it out when absent). */
  title_id?: TitleIdRef;
  /** The console content id (PKG header, `param.sfo`, `param.json`, XVC);
   * Rust leaves it out when absent. */
  content_id?: string;
  /** The `.m3u` a loose multi-disc set gets (Rust leaves it out when
   * absent). */
  playlist?: Playlist;
  /** Set only when the optional Switch catalogue named the title
   * (`titledb` or `nlib`, adjustments D7); Rust leaves it out otherwise. */
  title_source?: TitleSource;
  /** What the item is to its game folder; Rust leaves it out when
   * absent. */
  game?: GameLink;
};

/** A playlist written for a loose multi-disc set: a new file, never over
 * an existing one, removed by undo. Paths relative to the target root. */
export interface Playlist {
  path: string;
  /** Each disc's sheet or image, in disc order: the playlist's lines. */
  discs: string[];
}

export interface OrganizeCounts {
  /** Items that move (automatically or once reviewed). */
  roms: number;
  systems: number;
  auto: number;
  review: number;
  left: number;
  junk: number;
  other_files: number;
  deep_folders: number;
  /** Bytes of the items that move. */
  bytes: number;
  /** Pile copies set aside as duplicates (Rust leaves it out when 0). */
  set_aside: number;
  /** Pile copies of games the library already holds, set aside or
   * reported (Rust leaves it out when 0). */
  in_library: number;
}

/** A dry run: what would move where, and why. */
export interface OrganizePlan {
  source_root: string;
  target_root: string;
  /** The folder holding the system folders. */
  roms_dir: string;
  /** Profile id of the target layout. */
  layout: string;
  mode: TransferMode;
  names: NameMode;
  items: OrganizeItem[];
  counts: OrganizeCounts;
  /** Relative to the source root, with the system they resolve to. */
  sorted_folders: [string, string][];
  deep_folders: string[];
  other_files: string[];
  warnings: PlanWarning[];
  broad_root: boolean;
  fat32: boolean;
  move_allowed: boolean;
  /** The layout the target already looks like, when not the chosen one. */
  target_detected: string | null;
  created_at: string;
  /** Duplicate groups with at least one pile copy (Rust leaves it out
   * when empty). */
  duplicates: DupGroupView[];
  /** Set-aside items go to the OS Trash (Rust leaves it out when false). */
  set_aside_trash: boolean;
  /** Rows that are an update or DLC of a game (Rust leaves it out when 0). */
  updates?: number;
  /** Services whose absence or used-up quota changes this plan (D28,
   * plan IP-2); Rust leaves it out when none. */
  integration_notices?: IntegrationNotice[];
  /** Games laid out for the emulator chosen for their system (mods spec
   * D16); Rust leaves it out when none. */
  translated?: TranslatedGameView[];
}

/** What to plan; missing fields take `settings.organize`. */
export interface OrganizeRequest {
  source: string;
  layout?: string | null;
  /** Where the system folders go (default: the source). */
  target?: string | null;
  /** System folders straight in the target, without the ROMs folder. */
  flat?: boolean | null;
  mode?: TransferMode | null;
  names?: NameMode | null;
  /** Cached catalogues only: no download, no online lookup. */
  offline?: boolean | null;
  /** Read the folder again instead of reusing the kept survey (files were
   * added or removed). Sent only by a refresh, never kept in a request. */
  rescan?: boolean;
  /** What the duplicates step does (missing: `dedupe.organize_step`). */
  duplicates?: OrganizeDedupeMode | null;
  /** Game folders for this run (missing: the layout's own value with the
   * user's override in `organize.game_folders`; game folders spec D1). */
  game_folders?: boolean | null;
  /** The emulator chosen per system for this run (mods spec D16); a
   * system left out uses the layout's default. */
  emulators?: Record<string, EmulatorChoice>;
}

/** The user's answer for one plan item. */
export type OrganizeDecision = { item: number } & (
  | { choice: 'accept' }
  | { choice: 'assign'; system: string }
  | { choice: 'leave' }
  /** «No» to a huge item: left as not confirmed, never remembered. */
  | { choice: 'decline' }
);

/** A quick, name-only look at a folder no layout was detected in. */
export interface FolderProbe {
  rom_like: number;
  partial_downloads: number;
  cloud_only: number;
  /** Names of folders that already resolve to a system. */
  sorted_folders: string[];
  writable: boolean;
  broad_root: boolean;
  /** Stopped after 5 000 entries: counts are "5 000+". */
  truncated: boolean;
}

/** A dry run kept by the backend: the UI sends back only its id and decisions. */
export interface OrganizePlanView {
  plan_id: number;
  plan: OrganizePlan;
}

export type OrganizePhase =
  'walk' | 'identify' | 'dats' | 'hash' | 'apply' | 'extract';

/** The `organize-progress` event (planning and applying). An `extract`
 * event also names the `item` being extracted and how many of the bytes
 * its decode pass goes through are done; an item's first event has
 * `bytes_done` 0 and its last `bytes_done === bytes_total`. */
export type OrganizeProgressEvent = {
  plan_id: number;
  done: number;
  total: number;
} & (
  | { phase: Exclude<OrganizePhase, 'extract'> }
  | {
      phase: 'extract';
      /** The plan item's id. */
      item: number;
      bytes_done: number;
      bytes_total: number;
      /** Entries staged so far, first in archive order (absent on an older wire). */
      entries_done?: number;
      /** The item's entries (the generated playlist is not one). */
      entries_total?: number;
    }
);

export type OrganizeSkipReason =
  | 'changed-since-plan'
  | 'source-missing'
  | 'already-present'
  | 'collision-set'
  | 'not-decided'
  | 'cancelled'
  /** The external tool the plan used is gone or another version now. */
  | 'tool-gone';

export type OrganizeItemStatus =
  | { status: 'moved' }
  | { status: 'copied' }
  /** A cross-volume move whose source could not be deleted: both exist. */
  | { status: 'copied-not-removed' }
  /** Every entry extracted and verified; in move mode with nothing left
   * in it, the archive went to the Trash. */
  | { status: 'extracted' }
  /** Extracted, but the archive meant for the Trash stays where it was. */
  | { status: 'extracted-kept' }
  /** A duplicate set aside in the pile's quarantine. */
  | { status: 'quarantined' }
  /** A duplicate sent to the Trash (`targets`: its places there). */
  | { status: 'trashed' }
  | { status: 'skipped'; reason: OrganizeSkipReason }
  | { status: 'failed'; kind: ItemErrorKind; detail: string };

/** One plan item and what happened to it. */
export type OrganizeItemOutcome = OrganizeItemStatus & {
  item: number;
  /** Where its files are now (absolute). */
  targets: string[];
  /** Its licences that stayed where they were (absolute): their rename
   * failed, so they were never copied. Rust leaves it out when none did. */
  licences_left?: string[];
};

/** What an apply (or resume) did. */
export interface OrganizeReport {
  job: string | null;
  moved: number;
  copied: number;
  /** Archives whose entries were extracted (Rust leaves it out when 0;
   * the adapter fills it in). */
  extracted: number;
  skipped: number;
  failed: number;
  /** Duplicates set aside or sent to the Trash (Rust leaves it out when
   * 0; the adapter fills it in). */
  set_aside: number;
  items: OrganizeItemOutcome[];
  /** Set when the run stopped on a full, read-only or vanished volume. */
  paused: PauseKind | null;
  pause_reason: ItemErrorKind | null;
  /** Items not attempted yet: what a resume does. */
  resumable_items: number[];
}

export type OrganizeJobState =
  'applying' | 'done' | 'paused' | 'cancelled' | 'failed';

/** What `organize_apply` refused to write without a confirmation
 * (`large-transfer`, spec D23): the decided plan's numbers. */
export interface LargeTransferDetail {
  /** Bytes written to the target (copies and extractions). */
  bytes: number;
  /** Items that write them (updates, DLC and archives count). */
  items: number;
  /** Copy mode; false: a move across volumes (copied, then removed). */
  copies: boolean;
  /** Free bytes on the target volume for this user; null when unknown. */
  free: number | null;
  /** The target volume's name, else the target folder's. */
  volume: string;
  /** Items asked one by one, in plan order (adjustments D15). */
  huge: HugeItem[];
  /** Bytes of every other item (D16b: asked as a whole only when they
   * alone reach the threshold). */
  rest: number;
}

/** One item asked on its own before apply (adjustments D15): it alone,
 * with the shadPS4 overlays that follow it, writes 10 GB or more, or it
 * extracts a folder game out of an archive. */
export interface HugeItem {
  /** `OrganizeItem.id`. */
  item: number;
  /** File or folder name in the pile. */
  name: string;
  system: string;
  /** Bytes it writes, its followers included. */
  bytes: number;
  /** Extracted out of an archive. */
  extract: boolean;
  /** Items left with it when it is declined. */
  followers: number[];
}

/** What `organize_apply` gets once the questions are answered (D16):
 * every accepted huge item, and `rest + Σ accepted bytes` from the
 * numbers the error carried. A declined item travels as a `decline`
 * decision instead. */
export interface LargeConfirm {
  bytes: number;
  items: number[];
}

/** An IPC error as data (inside a view). */
export interface WireError {
  code: ErrorCode;
  detail?: string | null;
}

/** The organise apply of the app (`organize-changed` event). */
export interface OrganizeJobView {
  job: string | null;
  plan_id: number;
  state: OrganizeJobState;
  progress: OrganizeProgressEvent | null;
  report: OrganizeReport | null;
  /** The folder the library opens from afterwards. */
  target_root: string;
  /** Profile id the library opens with. */
  layout: string;
  /** Why it failed (`failed` only). */
  error: WireError | null;
}

/** Rust `UndoReport` (paths, unlike the fill `UndoView` counts). */
export interface OrganizeUndoReport {
  job: string;
  removed: string[];
  restored: string[];
  skipped_modified: string[];
  skipped_occupied: string[];
  dirs_removed: string[];
  /** Folders the job created that undo kept because they hold something
   * of the user's (Rust leaves it out when empty). */
  dirs_kept?: string[];
  /** Archives sent to the Trash that could not be brought back: their
   * extracted files stay (Rust leaves it out when empty; the adapter
   * fills it in). */
  archive_lost: string[];
}

/** One report per job undone (newest first) and the later jobs included. */
export interface OrganizeUndoView {
  report: OrganizeUndoReport[];
  later_jobs: JobHistoryItem[];
}

/* Wire forms: what Rust actually sends. It leaves `extracted` out of a
 * report when nothing was extracted and `archive_lost` out of an undo
 * report when empty, and plans exported before archives have no
 * `transfer`/`disc`. The Tauri adapter fills them in, so the app only
 * sees the types above. */

/** `T` with the keys `K` optional, member by member of a union. */
type WithOptional<T, K extends PropertyKey> = T extends unknown
  ? Omit<T, K & keyof T> & Partial<Pick<T, K & keyof T>>
  : never;

export type OrganizeItemWire = WithOptional<OrganizeItem, 'transfer' | 'disc'>;

export type OrganizeCountsWire = WithOptional<
  OrganizeCounts,
  'set_aside' | 'in_library'
>;

export type OrganizePlanWire = Omit<
  OrganizePlan,
  'items' | 'counts' | 'duplicates' | 'set_aside_trash'
> & {
  items: OrganizeItemWire[];
  counts: OrganizeCountsWire;
  duplicates?: DupGroupView[];
  set_aside_trash?: boolean;
};

export interface OrganizePlanViewWire {
  plan_id: number;
  plan: OrganizePlanWire;
}

export type OrganizeReportWire = WithOptional<
  OrganizeReport,
  'extracted' | 'set_aside'
>;

export type OrganizeJobViewWire = Omit<OrganizeJobView, 'report'> & {
  report: OrganizeReportWire | null;
};

export type OrganizeUndoReportWire = WithOptional<
  OrganizeUndoReport,
  'archive_lost'
>;

export interface OrganizeUndoViewWire {
  report: OrganizeUndoReportWire[];
  later_jobs: JobHistoryItem[];
}
