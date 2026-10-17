import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';

import type { AppInfo } from './appInfo';
import type { Backend } from './backend';
import { BackendError, toBackendError } from './errors';
import { sniffImageMime } from './sniff';
import type {
  ArtImage,
  CandidateView,
  CredentialStatus,
  EntriesUpdated,
  EntryArtUpdate,
  FolderProbe,
  IdentityView,
  JobHistoryItem,
  JobItemView,
  JobProgressEvent,
  JobView,
  KeyCheck,
  LibrarySettings,
  LibraryView,
  OrganizeJobView,
  OrganizePlanView,
  OrganizeProgressEvent,
  OrganizeUndoView,
  ProfileInfo,
  ProfilePreview,
  ProfileSaveOutcome,
  RecentLibrary,
  ScanProgress,
  Settings,
  SettingsUpdate,
  SettingsView,
  SourceInfo,
  UndoView,
  UpdateCheck,
  UpdateInstall,
  UpdateProgress,
  Volume,
} from './types';

export const SCAN_PROGRESS_EVENT = 'library-scan-progress';
export const JOB_PROGRESS_EVENT = 'fill-job-progress';
export const JOB_CHANGED_EVENT = 'fill-job-changed';
export const ENTRIES_UPDATED_EVENT = 'library-entries-updated';
export const SETTINGS_CHANGED_EVENT = 'settings-changed';
export const UPDATE_PROGRESS_EVENT = 'update-progress';
export const ORGANIZE_PROGRESS_EVENT = 'organize-progress';
export const ORGANIZE_CHANGED_EVENT = 'organize-changed';

async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toBackendError(e);
  }
}

/** A command returning image bytes; only PNG/JPEG/WebP/GIF become images. */
async function image(
  cmd: string,
  args: Record<string, unknown>,
): Promise<ArtImage> {
  const buffer = await call<ArrayBuffer>(cmd, args);
  const bytes = new Uint8Array(buffer);
  const mime = sniffImageMime(bytes);
  if (!mime) throw new BackendError('art-unreadable');
  return { bytes, mime };
}

export function createTauriBackend(): Backend {
  return {
    kind: 'tauri',
    appInfo: () => call<AppInfo>('app_info'),
    openLibrary: (path, profileId) =>
      call<LibraryView>('open_library', { path, profileId: profileId ?? null }),
    onScanProgress: (cb) =>
      listen<ScanProgress>(SCAN_PROGRESS_EVENT, (e) => cb(e.payload)),
    recentLibraries: () => call<RecentLibrary[]>('recent_libraries'),
    forgetRecentLibrary: (path) =>
      call<RecentLibrary[]>('forget_recent_library', { path }),
    mountedVolumes: () => call<Volume[]>('mounted_volumes'),
    librarySettings: () => call<LibrarySettings>('library_settings'),
    setLibrarySettings: (s) =>
      call<LibrarySettings>('set_library_settings', {
        ignoredDirs: s.ignored_dirs,
      }),
    readArt: (generation, entryId, media) =>
      image('read_art', { generation, entryId, media }),
    identifyEntry: (generation, entryId) =>
      call<IdentityView>('identify_entry', { generation, entryId }),
    revealEntry: (generation, entryId) =>
      call<void>('reveal_entry', { generation, entryId }),
    async pickFolder() {
      const picked = await open({ directory: true, multiple: false });
      return typeof picked === 'string' ? picked : null;
    },
    onFolderDrop: (cb) =>
      getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type !== 'drop') return;
        const first = event.payload.paths[0];
        if (first) cb(first);
      }),
    startFill: (generation, request) =>
      call<JobView>('start_fill', { generation, request }),
    fillJob: () => call<JobView | null>('fill_job'),
    fillJobItems: (job, filter) =>
      call<JobItemView[]>('fill_job_items', { job, filter }),
    decideFill: (job, decisions) =>
      call<JobView>('decide_fill', { job, decisions }),
    runFill: (job) => call<JobView>('run_fill', { job }),
    resumeFill: (job) => call<JobView>('resume_fill', { job }),
    cancelFill: (job) => call<JobView>('cancel_fill', { job }),
    candidateArt: (job, index, candidate) =>
      image('candidate_art', { job, index, candidate }),
    undoLastJob: (generation) =>
      call<UndoView>('undo_last_job', { generation }),
    jobHistory: () => call<JobHistoryItem[]>('job_history'),
    onJobProgress: (cb) =>
      listen<JobProgressEvent>(JOB_PROGRESS_EVENT, (e) => cb(e.payload)),
    onJobChanged: (cb) =>
      listen<JobView>(JOB_CHANGED_EVENT, (e) => cb(e.payload)),
    onEntriesUpdated: (cb) =>
      listen<EntriesUpdated>(ENTRIES_UPDATED_EVENT, (e) => cb(e.payload)),
    entryCandidates: (generation, entryId, media) =>
      call<CandidateView[]>('entry_candidates', { generation, entryId, media }),
    entryCandidateArt: (generation, entryId, media, candidate) =>
      image('entry_candidate_art', { generation, entryId, media, candidate }),
    setEntryArt: (generation, entryId, media, candidate) =>
      call<EntryArtUpdate[]>('set_entry_art', {
        generation,
        entryId,
        media,
        candidate,
      }),
    setEntryArtFromFile: (generation, entryId, media) =>
      call<EntryArtUpdate[] | null>('set_entry_art_from_file', {
        generation,
        entryId,
        media,
      }),
    removeEntryArt: (generation, entryId, media) =>
      call<EntryArtUpdate[]>('remove_entry_art', {
        generation,
        entryId,
        media,
      }),
    exportAudit: (generation, format) =>
      call<string | null>('export_audit', { generation, format }),
    exportJobReport: (job, format) =>
      call<string | null>('export_job_report', { job, format }),
    probeFolder: (path) => call<FolderProbe>('probe_folder', { path }),
    organizePlan: (request, decisions) =>
      call<OrganizePlanView>('organize_plan', {
        request,
        decisions: decisions ?? null,
      }),
    organizeCancel: (planId) => call<void>('organize_cancel', { planId }),
    organizeApply: (planId, decisions) =>
      call<OrganizeJobView>('organize_apply', { planId, decisions }),
    organizeResume: (job) => call<OrganizeJobView>('organize_resume', { job }),
    organizeCancelApply: () => call<void>('organize_cancel_apply'),
    organizeUndo: (job, includeLater) =>
      call<OrganizeUndoView>('organize_undo', { job, includeLater }),
    organizeExportPlan: (planId) =>
      call<string | null>('organize_export_plan', { planId }),
    onOrganizeProgress: (cb) =>
      listen<OrganizeProgressEvent>(ORGANIZE_PROGRESS_EVENT, (e) =>
        cb(e.payload),
      ),
    onOrganizeChanged: (cb) =>
      listen<OrganizeJobView>(ORGANIZE_CHANGED_EVENT, (e) => cb(e.payload)),
    getSettings: () => call<SettingsView>('get_settings'),
    updateSettings: (patch) =>
      call<SettingsUpdate>('update_settings', { patch }),
    resetSettings: () => call<SettingsView>('reset_settings'),
    onSettingsChanged: (cb) =>
      listen<Settings>(SETTINGS_CHANGED_EVENT, (e) => cb(e.payload)),
    openConfigFolder: () => call<void>('open_config_folder'),
    openLogsFolder: () => call<void>('open_logs_folder'),
    openLink: (link) => call<void>('open_link', { link }),
    cacheUsage: () => call<number>('cache_usage'),
    clearCache: () => call<number>('clear_cache'),
    listProfiles: () => call<ProfileInfo[]>('list_profiles'),
    saveProfile: (profile, replaces) =>
      call<ProfileSaveOutcome>('save_profile', { profile, replaces }),
    deleteProfile: (id) => call<ProfileInfo[]>('delete_profile', { id }),
    previewProfile: (profile, example) =>
      call<ProfilePreview>('preview_profile', { profile, example }),
    credentialStatus: () => call<CredentialStatus>('credential_status'),
    setCredential: (key, value) =>
      call<CredentialStatus>('set_credential', { key, value }),
    clearCredential: (key) =>
      call<CredentialStatus>('clear_credential', { key }),
    sourcesInfo: () => call<SourceInfo[]>('sources_info'),
    testSourceKey: (source) => call<KeyCheck>('test_source_key', { source }),
    searchEntryArt: (generation, entryId, media, query) =>
      call<CandidateView[]>('search_entry_art', {
        generation,
        entryId,
        media,
        query,
      }),
    entrySearchArt: (generation, entryId, media, candidate) =>
      image('entry_search_art', { generation, entryId, media, candidate }),
    setEntryArtFromSearch: (generation, entryId, media, candidate) =>
      call<EntryArtUpdate[]>('set_entry_art_from_search', {
        generation,
        entryId,
        media,
        candidate,
      }),
    clearIdentityOverride: (generation, entryId) =>
      call<void>('clear_identity_override', { generation, entryId }),
    checkForUpdates: (trigger) =>
      call<UpdateCheck>('check_update', { trigger }),
    downloadUpdate: () => call<void>('download_update'),
    onUpdateProgress: (cb) =>
      listen<UpdateProgress>(UPDATE_PROGRESS_EVENT, (e) => cb(e.payload)),
    installUpdate: () => call<UpdateInstall>('install_update'),
    restartApp: () => call<void>('restart_app'),
  };
}
