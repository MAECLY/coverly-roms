/**
 * The one door from the UI to a backend: the Tauri app, or — in a plain
 * browser (`pnpm dev`) — the deterministic mock.
 */
import { hasTauriIpc, type AppInfo } from './appInfo';
import type {
  ArtImage,
  CandidateView,
  CredentialKey,
  CredentialStatus,
  DecisionIn,
  EntriesUpdated,
  EntryArtUpdate,
  ExportFormat,
  FillRequest,
  FolderProbe,
  IdentityView,
  ItemFilter,
  JobHistoryItem,
  JobItemView,
  JobProgressEvent,
  JobView,
  KeyCheck,
  LibrarySettings,
  LibraryView,
  LinkId,
  Media,
  OrganizeDecision,
  OrganizeJobView,
  OrganizePlanView,
  OrganizeProgressEvent,
  OrganizeRequest,
  OrganizeUndoView,
  PreviewExample,
  ProfileFile,
  ProfileInfo,
  SystemInfo,
  ProfilePreview,
  ProfileSaveOutcome,
  RecentLibrary,
  ScanProgress,
  Settings,
  SettingsPatch,
  SettingsUpdate,
  SettingsView,
  SourceId,
  SourceInfo,
  UndoView,
  UpdateCheck,
  UpdateInstall,
  UpdateProgress,
  UpdateTrigger,
  Volume,
} from './types';

export interface Backend {
  readonly kind: 'tauri' | 'mock';
  /** Product name and the app and core versions. */
  appInfo(): Promise<AppInfo>;
  openLibrary(path: string, profileId?: string): Promise<LibraryView>;
  onScanProgress(cb: (p: ScanProgress) => void): Promise<() => void>;
  recentLibraries(): Promise<RecentLibrary[]>;
  forgetRecentLibrary(path: string): Promise<RecentLibrary[]>;
  /** Drives and SD cards connected now (a read-only listing). */
  mountedVolumes(): Promise<Volume[]>;
  librarySettings(): Promise<LibrarySettings>;
  setLibrarySettings(s: LibrarySettings): Promise<LibrarySettings>;
  readArt(generation: number, entryId: number, media: Media): Promise<ArtImage>;
  identifyEntry(generation: number, entryId: number): Promise<IdentityView>;
  revealEntry(generation: number, entryId: number): Promise<void>;
  pickFolder(): Promise<string | null>;
  onFolderDrop(cb: (path: string) => void): Promise<() => void>;
  startFill(generation: number, request: FillRequest): Promise<JobView>;
  fillJob(): Promise<JobView | null>;
  fillJobItems(job: number, filter: ItemFilter): Promise<JobItemView[]>;
  decideFill(job: number, decisions: DecisionIn[]): Promise<JobView>;
  runFill(job: number): Promise<JobView>;
  resumeFill(job: number): Promise<JobView>;
  cancelFill(job: number): Promise<JobView>;
  candidateArt(
    job: number,
    index: number,
    candidate: number,
  ): Promise<ArtImage>;
  undoLastJob(generation: number): Promise<UndoView>;
  jobHistory(): Promise<JobHistoryItem[]>;
  onJobProgress(cb: (e: JobProgressEvent) => void): Promise<() => void>;
  onJobChanged(cb: (v: JobView) => void): Promise<() => void>;
  onEntriesUpdated(cb: (e: EntriesUpdated) => void): Promise<() => void>;
  entryCandidates(
    generation: number,
    entryId: number,
    media: Media,
  ): Promise<CandidateView[]>;
  entryCandidateArt(
    generation: number,
    entryId: number,
    media: Media,
    candidate: number,
  ): Promise<ArtImage>;
  setEntryArt(
    generation: number,
    entryId: number,
    media: Media,
    candidate: number,
  ): Promise<EntryArtUpdate[]>;
  /** Opens a native image picker; null when the user cancels. */
  setEntryArtFromFile(
    generation: number,
    entryId: number,
    media: Media,
  ): Promise<EntryArtUpdate[] | null>;
  removeEntryArt(
    generation: number,
    entryId: number,
    media: Media,
  ): Promise<EntryArtUpdate[]>;
  /** Saves through a native dialog; the file name, or null when cancelled. */
  exportAudit(generation: number, format: ExportFormat): Promise<string | null>;
  exportJobReport(job: number, format: ExportFormat): Promise<string | null>;

  /* Organising a loose pile (organise spec §4, §11): the backend keeps
   * each dry run; the UI sends back only its id and the decisions. */
  /** A quick, name-only look at a folder no layout was detected in. */
  probeFolder(path: string): Promise<FolderProbe>;
  /** Plans organising `request.source` (progress via `onOrganizeProgress`). */
  organizePlan(
    request: OrganizeRequest,
    decisions?: OrganizeDecision[],
  ): Promise<OrganizePlanView>;
  /** Stops planning `planId`; its `organizePlan` rejects with `cancelled`. */
  organizeCancel(planId: number): Promise<void>;
  /** Starts applying plan `planId`; the end arrives via `onOrganizeChanged`.
   * `job-running` while a fill job runs or is paused (and fills refuse meanwhile). */
  organizeApply(
    planId: number,
    decisions: OrganizeDecision[],
  ): Promise<OrganizeJobView>;
  /** Continues a paused or interrupted organise job into its journal. */
  organizeResume(job: string): Promise<OrganizeJobView>;
  /** Stops the running organise apply before its next item. */
  organizeCancelApply(): Promise<void>;
  /** Undoes organise job `job`; newer jobs under it refuse with
   * `later-jobs` unless `includeLater`. */
  organizeUndo(job: string, includeLater: boolean): Promise<OrganizeUndoView>;
  /** Saves plan `planId` as JSON through a native dialog; null when cancelled. */
  organizeExportPlan(planId: number): Promise<string | null>;
  onOrganizeProgress(
    cb: (e: OrganizeProgressEvent) => void,
  ): Promise<() => void>;
  onOrganizeChanged(cb: (v: OrganizeJobView) => void): Promise<() => void>;

  /* Settings (§9): field problems come back as data, not errors. */
  getSettings(): Promise<SettingsView>;
  updateSettings(patch: SettingsPatch): Promise<SettingsUpdate>;
  /** Defaults again; the recent libraries are kept. */
  resetSettings(): Promise<SettingsView>;
  onSettingsChanged(cb: (s: Settings) => void): Promise<() => void>;
  /** Folders and links are fixed ids: the webview never names a path or URL. */
  openConfigFolder(): Promise<void>;
  openLogsFolder(): Promise<void>;
  openLink(link: LinkId): Promise<void>;
  /** Bytes used by the downloaded-art cache. */
  cacheUsage(): Promise<number>;
  /** Clears the cache (refused while a job runs); the bytes freed. */
  clearCache(): Promise<number>;

  /* Layout profiles: built-ins read-only, custom ones editable. */
  listProfiles(): Promise<ProfileInfo[]>;
  /** The systems Coverly knows, by name (organise pickers). */
  listSystems(): Promise<SystemInfo[]>;
  /** `replaces` = the id the profile had (a rename), null for a new one. */
  saveProfile(
    profile: ProfileFile,
    replaces: string | null,
  ): Promise<ProfileSaveOutcome>;
  deleteProfile(id: string): Promise<ProfileInfo[]>;
  previewProfile(
    profile: ProfileFile,
    example: PreviewExample,
  ): Promise<ProfilePreview>;

  /* Source credentials: values go in once and never come back. */
  credentialStatus(): Promise<CredentialStatus>;
  setCredential(key: CredentialKey, value: string): Promise<CredentialStatus>;
  clearCredential(key: CredentialKey): Promise<CredentialStatus>;

  /* Sources (M6): which are built/ready, and a cheap key check; keys never come back. */
  sourcesInfo(): Promise<SourceInfo[]>;
  testSourceKey(source: SourceId): Promise<KeyCheck>;

  /* Drawer "Search by title" (manual identity, D32). */
  searchEntryArt(
    generation: number,
    entryId: number,
    media: Media,
    query: string,
  ): Promise<CandidateView[]>;
  entrySearchArt(
    generation: number,
    entryId: number,
    media: Media,
    candidate: number,
  ): Promise<ArtImage>;
  setEntryArtFromSearch(
    generation: number,
    entryId: number,
    media: Media,
    candidate: number,
  ): Promise<EntryArtUpdate[]>;
  clearIdentityOverride(generation: number, entryId: number): Promise<void>;

  /* In-app updates (M7): driven from Rust, never from the webview. */
  /** Checks the channel's manifest; a `startup` check with "check on start"
   * off requests nothing and answers `skipped`. */
  checkForUpdates(trigger: UpdateTrigger): Promise<UpdateCheck>;
  /** Downloads and verifies the offered update (progress via `onUpdateProgress`). */
  downloadUpdate(): Promise<void>;
  onUpdateProgress(cb: (p: UpdateProgress) => void): Promise<() => void>;
  /** Installs the downloaded update; `job-running` while a fill job runs or is paused. */
  installUpdate(): Promise<UpdateInstall>;
  /** Restarts into the installed update; `job-running` while a fill job runs or is paused. */
  restartApp(): Promise<void>;
}

let current: Backend | null = null;
let pending: Promise<Backend> | null = null;

async function createBackend(): Promise<Backend> {
  if (hasTauriIpc()) {
    const { createTauriBackend } = await import('./tauriBackend');
    return createTauriBackend();
  }
  const { createMockBackend } = await import('./mock/mockBackend');
  return createMockBackend();
}

/**
 * The Tauri backend inside the app, the mock in a plain browser. One
 * instance per page: concurrent first calls share the same promise (the
 * mock is stateful).
 */
export function getBackend(): Promise<Backend> {
  if (current) return Promise.resolve(current);
  if (pending) return pending;
  const created: Promise<Backend> = createBackend().then((backend) => {
    // A `setBackend` call while this was loading wins over it.
    if (pending !== created) return current ?? backend;
    current = backend;
    pending = null;
    return backend;
  });
  pending = created;
  return created;
}

/** Tests (and only tests) install a backend; `null` resets the choice. */
export function setBackend(backend: Backend | null): void {
  current = backend;
  pending = null;
}
