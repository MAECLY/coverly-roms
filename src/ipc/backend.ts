/**
 * The one door from the UI to a backend: the Tauri app, or — in a plain
 * browser (`pnpm dev`) — the deterministic mock.
 */
import { hasTauriIpc, type AppInfo } from './appInfo';
import type { DropPoint } from './dropTarget';
import type {
  AddChoice,
  AddFinishedEvent,
  AddPlanView,
  AddProgressEvent,
  AddStarted,
  ArtImage,
  CategoryRef,
  CandidateView,
  ChildArtReadView,
  ChildMedia,
  ChildView,
  CatalogStatus,
  CreditLink,
  CredentialKey,
  CredentialStatus,
  DecisionIn,
  DedupeJobView,
  DedupePlanView,
  DedupeProgressEvent,
  DedupeRequest,
  EntriesUpdated,
  EntryArtUpdate,
  ExportFormat,
  ExtrasDropEvent,
  ExtrasMedia,
  ExtrasTextView,
  ExtrasUndone,
  ExtrasView,
  ExtrasWriteView,
  FillRequest,
  FolderProbe,
  GameDetailsView,
  IdentityView,
  IntegrationTestResult,
  IntegrationView,
  ItemFilter,
  JobHistoryItem,
  JobItemView,
  JobProgressEvent,
  JobView,
  KeyCheck,
  LargeConfirm,
  LibrarySettings,
  LibraryView,
  LinkId,
  MediaView,
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
  /** The game panel's lazy, offline facts (part C D3). `request` comes
   * back in the answer: drop one that is not the latest. */
  gameDetails(
    generation: number,
    entryId: number,
    request: number,
  ): Promise<GameDetailsView>;
  /** Makes this revision the default of its group in this library (D7);
   * answers the library's choices (version group → `rel_path`). */
  setDefaultRevision(
    generation: number,
    entryId: number,
  ): Promise<Record<string, string>>;
  /** «Leer el arte incluido»: the art the game's updates and DLC carry in
   * the clear, into their empty slots (an undoable job, D11 b). */
  readChildArt(generation: number, entryId: number): Promise<ChildArtReadView>;
  /** «Buscar arte» on an update or DLC: a title search for child
   * `childIndex` (ScreenScraper and SteamGridDB results only). Nothing is
   * written. */
  searchChildArt(
    generation: number,
    entryId: number,
    childIndex: number,
    query: string,
  ): Promise<CandidateView[]>;
  /** A child search result, as it would be written. */
  childSearchArt(
    generation: number,
    entryId: number,
    childIndex: number,
    candidate: number,
  ): Promise<ArtImage>;
  /** Writes result `candidate` of the child's last search into its
   * `slot`, on an explicit pick only. Answers the game's children again. */
  setChildArtFromSearch(
    generation: number,
    entryId: number,
    childIndex: number,
    slot: ChildMedia,
    candidate: number,
  ): Promise<ChildView[]>;
  /** One art slot of an update or DLC, by index (never a path). */
  readChildArtBytes(
    generation: number,
    entryId: number,
    childIndex: number,
    slot: ChildMedia,
  ): Promise<ArtImage>;
  pickFolder(): Promise<string | null>;
  /**
   * A folder dropped from the OS, to open as a library, with where it
   * landed (CSS pixels). Drops over the Extras section or while a dialog
   * or the viewer is open are not reported (part D D11).
   */
  onFolderDrop(
    cb: (path: string, point: DropPoint) => void,
  ): Promise<() => void>;
  /* A game's Extras/ (part D). Files by `(rev, index)` of the last
   * listing, drops by token, adds by plan: never a path (D22). A write
   * answers the next listing; a stale `rev` is `extras-changed`. */
  extrasList(generation: number, entryId: number): Promise<ExtrasView>;
  /** Renames file `index` to the typed `stem`; its extension stays. */
  extrasRename(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
    stem: string,
  ): Promise<ExtrasWriteView>;
  extrasMove(
    generation: number,
    entryId: number,
    rev: number,
    files: number[],
    to: CategoryRef,
  ): Promise<ExtrasWriteView>;
  /** To the Trash, or set aside where the drive has none; never deleted. */
  extrasTrash(
    generation: number,
    entryId: number,
    rev: number,
    files: number[],
  ): Promise<ExtrasWriteView>;
  extrasSetTags(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
    tags: string[],
  ): Promise<ExtrasWriteView>;
  /** «Olvidar» record `missing` of «No encontrados». */
  extrasForget(
    generation: number,
    entryId: number,
    rev: number,
    missing: number,
  ): Promise<ExtrasWriteView>;
  extrasCategoryCreate(
    generation: number,
    entryId: number,
    rev: number,
    name: string,
  ): Promise<ExtrasWriteView>;
  extrasCategoryRename(
    generation: number,
    entryId: number,
    rev: number,
    from: string,
    to: string,
  ): Promise<ExtrasWriteView>;
  /** Its files to the Trash, then its emptied folder. */
  extrasCategoryRemove(
    generation: number,
    entryId: number,
    rev: number,
    name: string,
  ): Promise<ExtrasWriteView>;
  /** Undoes Extras job `job` of this game; answers its listing after. */
  extrasUndo(
    generation: number,
    entryId: number,
    job: string,
  ): Promise<ExtrasUndone>;
  /** «Añadir archivos»: the native picker (opened by Rust), then the plan
   * the add dialog shows; `null` when cancelled. */
  extrasAddPick(
    generation: number,
    entryId: number,
  ): Promise<AddPlanView | null>;
  /** The plan of OS drop `token` (a drop on no category). */
  extrasAddPlan(
    generation: number,
    entryId: number,
    token: number,
  ): Promise<AddPlanView>;
  /** Copies the chosen files of `plan`, each to its category. */
  extrasAddCommit(
    generation: number,
    entryId: number,
    rev: number,
    plan: number,
    choices: AddChoice[],
  ): Promise<AddStarted>;
  /** Copies what OS drop `token` recorded into `category`. */
  extrasAddDropped(
    generation: number,
    entryId: number,
    rev: number,
    token: number,
    category: CategoryRef,
  ): Promise<AddStarted>;
  /** «Cancelar» on background add `add`: stops after the current file. */
  extrasAddCancel(add: number): Promise<boolean>;
  /** Grants file `index` to the viewer and answers its URL. */
  extrasMedia(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
  ): Promise<ExtrasMedia>;
  extrasReadText(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
  ): Promise<ExtrasTextView>;
  extrasReveal(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
  ): Promise<void>;
  /** Allow-listed types only; anything else is refused (reveal-only). */
  extrasOpenExternal(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
  ): Promise<void>;
  /** OS drops Rust recorded by token, `position` in CSS pixels (D11). */
  onExtrasDrop(cb: (e: ExtrasDropEvent) => void): Promise<() => void>;
  /** Where an OS drag is over the window, in CSS pixels; `null` when it
   * left or dropped. HTML5 `dragover` never fires while Tauri handles
   * drops, so this drives the drop zones' highlight (D11). */
  onExtrasDragOver(cb: (point: DropPoint | null) => void): Promise<() => void>;
  onExtrasProgress(cb: (e: AddProgressEvent) => void): Promise<() => void>;
  /** A background add ended. */
  onExtrasAdded(cb: (e: AddFinishedEvent) => void): Promise<() => void>;
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
   * `job-running` while a fill job runs or is paused (and fills refuse meanwhile).
   * `large-transfer` (with the numbers, `huge` and `rest`) when the decided
   * plan writes 10 GB or more or has huge items, and `confirm` does not name
   * every huge item or is below its bytes (spec D23, adjustments D16). A
   * declined huge item is sent as a `decline` decision. */
  organizeApply(
    planId: number,
    decisions: OrganizeDecision[],
    confirm?: LargeConfirm,
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
  /** The icon a pile's update, DLC or demo carries in the clear, read
   * from its source file for its row (addon views D15); by plan and item
   * id, never a path. Nothing is written; rejects with `art-missing` when
   * the package has none in the clear. */
  organizeChildThumb(planId: number, itemId: number): Promise<ArtImage>;
  onOrganizeProgress(
    cb: (e: OrganizeProgressEvent) => void,
  ): Promise<() => void>;
  onOrganizeChanged(cb: (v: OrganizeJobView) => void): Promise<() => void>;

  /* Duplicates (duplicates spec §11, §12): the backend keeps each plan
   * under its id; the UI sends back only the id and its picks. Undo goes
   * through `organizeUndo`. */
  /** Looks for duplicates in the library at `root` (a dry run; progress
   * via `onDedupeProgress`). Missing options take `settings.dedupe`. */
  dedupePlan(root: string, opts?: DedupeRequest): Promise<DedupePlanView>;
  /** Stops planning `planId`; its `dedupePlan` rejects with `cancelled`. */
  dedupeCancel(planId: number): Promise<void>;
  /** Plan `planId` with other levels, rules or destination (nothing is
   * read again). */
  dedupeRegroup(planId: number, opts: DedupeRequest): Promise<DedupePlanView>;
  /** Keeps member `member` of group `group` (remembered for next time). */
  dedupeSetKeeper(
    planId: number,
    group: number,
    member: number,
  ): Promise<DedupePlanView>;
  /** Leaves group `group` alone (or takes it back). */
  dedupeSkipGroup(
    planId: number,
    group: number,
    skip: boolean,
  ): Promise<DedupePlanView>;
  /** Leaves every group of `system` alone (or takes them back). */
  dedupeSkipSystem(
    planId: number,
    system: string,
    skip: boolean,
  ): Promise<DedupePlanView>;
  /** Forgets the keepers chosen for `root` (`systems`, or all); how many. */
  dedupeResetChoices(root: string, systems?: string[]): Promise<number>;
  /** Sets the extras of plan `planId` aside; the end arrives via
   * `onDedupeChanged`. `job-running` while a fill or organise job writes. */
  dedupeApply(planId: number): Promise<DedupeJobView>;
  /** Stops the running duplicates apply between groups. */
  dedupeCancelApply(): Promise<void>;
  /** The current (or last) duplicates apply of this session. */
  dedupeJob(): Promise<DedupeJobView | null>;
  /** Continues a paused or interrupted duplicates job into its journal. */
  dedupeResume(job: string): Promise<DedupeJobView>;
  /** Saves plan `planId` as JSON through a native dialog; null when cancelled. */
  dedupeExportPlan(planId: number): Promise<string | null>;
  /** Shows `path` in Finder / Explorer: only inside the open library or a
   * duplicates plan's library or quarantine. */
  revealPath(path: string): Promise<void>;
  /** A duplicate group's cover (`member` null) or a copy's own art;
   * rejects `art-missing` when there is none. Offline only. */
  dedupeArt(
    planId: number,
    group: number,
    member: number | null,
    media: MediaView,
  ): Promise<ArrayBuffer>;
  onDedupeProgress(cb: (e: DedupeProgressEvent) => void): Promise<() => void>;
  onDedupeChanged(cb: (v: DedupeJobView) => void): Promise<() => void>;

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
  /** Opens the site or licence of credit entry `id` (the URL comes from
   * the compiled-in credits file); `bad-request` for an unknown id or a
   * missing link (adjustments D27). */
  openCredit(id: string, which: CreditLink): Promise<void>;

  /* The optional Switch name catalogue (adjustments D2–D10). `locale` is
   * the language the UI shows (the region file follows it while the
   * language setting is automatic). */
  catalogStatus(locale?: string): Promise<CatalogStatus>;
  /** Downloads now or refreshes when due (progress via
   * `onCatalogProgress`); `bad-request` while the setting is off. */
  catalogUpdate(locale?: string): Promise<CatalogStatus>;
  onCatalogProgress(cb: (s: CatalogStatus) => void): Promise<() => void>;
  /** Stops a running catalogue download. */
  catalogCancel(): Promise<void>;
  /** «Borrar catálogo»; the status after it. */
  catalogRemove(): Promise<CatalogStatus>;
  /** The cached eShop icon of title `tid` (16 hex digits), fetched on
   * demand while «Iconos de Switch» is on; `art-missing` otherwise. */
  catalogIcon(tid: string): Promise<ArtImage>;
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

  /* «Integraciones» (integrations spec): one view per working integration
   * of the registry, with presence flags and a state, never a value.
   * Opening the tab sends nothing over the network (D9). */
  integrationsView(): Promise<IntegrationView[]>;
  /** «Probar conexión»: one cheap request with the stored keys; the
   * verdict and any quota become the card's state for the session. */
  testIntegration(id: string): Promise<IntegrationTestResult>;
  /** «Olvidar credenciales»: clears every field of `id`; the fresh view. */
  forgetIntegration(id: string): Promise<IntegrationView[]>;
  /** Opens the service's official account or API page (the URL comes from
   * the registry in Rust, never from the webview). */
  openIntegrationHelp(id: string): Promise<void>;

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

/** Dev helpers the mock puts on `window` (never in the app). */
type MockWindow = Window & {
  mockOrganizeTouch?: (
    path: string,
    change: import('./mock/mockOrganize').MockOrganizeTouch,
  ) => void;
  mockDedupeScenario?: (
    patch: Partial<import('./mock/mockDedupe').MockDedupeScenario>,
  ) => void;
  /** Paths the duplicates screen asked to reveal (a copy). */
  mockRevealedPaths?: () => string[];
  /** `dedupeArt` calls as `plan:group:member|g:media` (a copy). */
  mockDedupeArtCalls?: () => string[];
  /** A simulated OS drop over the Extras section (part D); its token. */
  mockExtrasDrop?: (drop: {
    names: string[];
    position: { x: number; y: number };
  }) => number;
  /** A simulated OS drag over the window (CSS pixels), `null` to leave. */
  mockExtrasDragOver?: (point: { x: number; y: number } | null) => void;
  /** Extras paths revealed and handed to another app (copies). */
  mockExtrasRevealed?: () => string[];
  mockExtrasOpened?: () => string[];
};

let current: Backend | null = null;
let pending: Promise<Backend> | null = null;

async function createBackend(): Promise<Backend> {
  if (hasTauriIpc()) {
    const { createTauriBackend } = await import('./tauriBackend');
    return createTauriBackend();
  }
  const { createMockBackend, readMockExtract } =
    await import('./mock/mockBackend');
  // `?mockExtract=fast|slow|<MB/s>`: how fast organise extracts (default 25 MB/s).
  const mock = createMockBackend({ extractRate: readMockExtract() });
  // The browser check changes a pile between scans from the console:
  // `mockOrganizeTouch('/Users/demo/Mixed', { add: 2, remove: 1 })`.
  // The duplicates screen's states from the console:
  // `mockDedupeScenario({ empty: true })`, `({ failPlan: 'volume-gone' })`,
  // `({ trashAvailable: false })`, `({ pauseAfter: 10 })`.
  // What the E2E checks: `mockRevealedPaths()`, `mockDedupeArtCalls()`.
  // An OS drop on the Extras section (CSS pixels):
  // `mockExtrasDrop({ names: ['Manual.pdf'], position: { x: 900, y: 400 } })`.
  if (typeof window !== 'undefined') {
    const w = window as MockWindow;
    w.mockOrganizeTouch = mock.organizeTouch;
    w.mockDedupeScenario = mock.dedupeScenario;
    w.mockRevealedPaths = () => [...mock.revealedPaths];
    w.mockDedupeArtCalls = () => [...mock.artCalls];
    w.mockExtrasDrop = mock.emitDrop;
    w.mockExtrasDragOver = mock.emitDragOver;
    w.mockExtrasRevealed = () => [...mock.extrasRevealed];
    w.mockExtrasOpened = () => [...mock.extrasOpened];
  }
  return mock;
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
