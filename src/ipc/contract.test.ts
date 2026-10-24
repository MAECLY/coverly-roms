import { describe, expect, it } from 'vitest';

import { MOCK_BUILTIN_PROFILES } from '@/ipc/mock/mockBackend';
import { BUILTIN_LAYOUTS, DEFAULT_SETTINGS, RANGES } from '@/lib/settingsRules';

import requests from './contract.requests.json';
import { ERROR_CODES } from './errors';
import sample from './contract.sample.json';
import {
  CREDENTIAL_KEYS,
  KEY_CHECKS,
  type PickReason,
  type SourceInfo,
  type CandidateView,
  type CredentialStatus,
  type FieldError,
  type ProfileInfo,
  type SettingsPatch,
  type DecisionIn,
  type EntriesUpdated,
  type EntryView,
  type FillRequest,
  type IdentityView,
  type ItemErrorKind,
  type JobHistoryItem,
  type JobItemView,
  type JobProgressEvent,
  type JobView,
  type LibraryView,
  type FolderProbe,
  type OrganizeItemOutcome,
  type LeftReason,
  type OrganizeCounts,
  type OrganizeDecision,
  type OrganizeItem,
  type OrganizeJobView,
  type OrganizePlan,
  type OrganizePlanView,
  type OrganizeProgressEvent,
  type OrganizeReport,
  type OrganizeRequest,
  type OrganizeUndoView,
  type OrganizeUndoReport,
  type ProfileView,
  type RecentLibrary,
  type ScanProgress,
  type SystemView,
  type UndoView,
  type UpdateCheck,
  type UpdateInstall,
  type UpdateOffer,
  type UpdateProgress,
  type Volume,
} from './types';

/** Compiles only when `K` lists every key of `T`. */
type AllKeys<T, K extends readonly (keyof T)[]> =
  Exclude<keyof T, K[number]> extends never ? K : never;

const VIEW_KEYS = [
  'generation',
  'root',
  'name',
  'profile',
  'systems',
  'entries',
  'warnings',
  'scan_ms',
  'loose_files',
] as const;
const ENTRY_KEYS = [
  'id',
  'system',
  'rel_path',
  'dir',
  'name',
  'ext',
  'size',
  'mtime',
  'regions',
  'art',
] as const;
const SYSTEM_KEYS = [
  'index',
  'folder',
  'dir',
  'system_id',
  'name',
  'roms',
] as const;
const PROFILE_KEYS = [
  'id',
  'name',
  'media',
  'detected',
  'evidence',
  'fitted',
  'gamelist',
] as const;
const PROGRESS_KEYS = [
  'generation',
  'done',
  'total',
  'folder',
  'roms',
] as const;
const RECENT_KEYS = [
  'path',
  'name',
  'profile_id',
  'roms',
  'opened_at',
  'available',
] as const;
const IDENTITY_KEYS = [
  'canonical',
  'method',
  'confidence',
  'title',
  'regions',
  'offline',
] as const;
const JOB_KEYS = [
  'id',
  'generation',
  'state',
  'progress',
  'paused',
  'plan',
  'decided',
  'skipped',
  'journal',
  'result',
  'error',
] as const;
const ITEM_KEYS = [
  'index',
  'entry_id',
  'folder',
  'name',
  'media',
  'action',
  'confidence',
  'note',
  'none_reason',
  'candidates',
  'replaces_existing',
  'decision',
  'outcome',
] as const;
const CANDIDATE_KEYS = [
  'index',
  'source',
  'name',
  'regions',
  'kind',
  'score',
  'confidence',
  'votes',
  'origin',
  'reason',
] as const;
const JOB_PROGRESS_KEYS = ['job', 'phase', 'done', 'total', 'label'] as const;
const UPDATED_KEYS = ['generation', 'entries'] as const;
const UNDO_KEYS = ['job', 'removed', 'restored', 'kept', 'entries'] as const;
const HISTORY_KEYS = [
  'job',
  'root',
  'created_at',
  'entries',
  'complete',
  'undone',
  'kind',
  'undoable',
] as const;
const UPDATE_OFFER_KEYS = ['version', 'notes', 'date'] as const;
const UPDATE_PROGRESS_KEYS = ['downloaded', 'total'] as const;
const UPDATE_INSTALL_KEYS = ['restart_required'] as const;
const VOLUME_KEYS = ['path', 'name'] as const;
const REQUEST_KEYS = [
  'entry_ids',
  'media',
  'strategy',
  'regions',
  'overwrite',
] as const;

const PROBE_KEYS = [
  'rom_like',
  'partial_downloads',
  'cloud_only',
  'sorted_folders',
  'writable',
  'broad_root',
  'truncated',
] as const;
const PLAN_VIEW_KEYS = ['plan_id', 'plan'] as const;
const PLAN_KEYS = [
  'source_root',
  'target_root',
  'roms_dir',
  'layout',
  'mode',
  'names',
  'items',
  'counts',
  'sorted_folders',
  'deep_folders',
  'other_files',
  'warnings',
  'broad_root',
  'fat32',
  'move_allowed',
  'target_detected',
  'created_at',
] as const;
const ORGANIZE_ITEM_KEYS = [
  'id',
  'sources',
  'kind',
  'size',
  'fps',
  'target',
  'set_targets',
  'system_id',
  'folder',
  'confidence',
  'method',
  'why',
  'alternatives',
  'rename',
  'notes',
  'companions',
  'catalogue_name',
  'transfer',
  'disc',
  'action',
] as const;
const COUNTS_KEYS = [
  'roms',
  'systems',
  'auto',
  'review',
  'left',
  'junk',
  'other_files',
  'deep_folders',
  'bytes',
] as const;
const ORGANIZE_PROGRESS_KEYS = ['plan_id', 'phase', 'done', 'total'] as const;
const ORGANIZE_JOB_KEYS = [
  'job',
  'plan_id',
  'state',
  'progress',
  'report',
  'target_root',
  'layout',
  'error',
] as const;
const ORGANIZE_REPORT_KEYS = [
  'job',
  'moved',
  'copied',
  'skipped',
  'failed',
  'items',
  'paused',
  'pause_reason',
  'resumable_items',
] as const;
const ORGANIZE_UNDO_KEYS = ['report', 'later_jobs'] as const;
const UNDO_REPORT_KEYS = [
  'job',
  'removed',
  'restored',
  'skipped_modified',
  'skipped_occupied',
  'dirs_removed',
] as const;
const ORGANIZE_REQUEST_KEYS = [
  'source',
  'layout',
  'target',
  'flat',
  'mode',
  'names',
  'offline',
  'rescan',
] as const;

/** Compiles only when `K` lists every member of the union `T`. */
type AllMembers<T, K extends readonly T[]> =
  Exclude<T, K[number]> extends never ? K : never;

const ITEM_ERROR_KINDS = [
  'no-art',
  'network',
  'decode',
  'write-denied',
  'disk-full',
  'name-invalid',
  'outside-root',
  'volume-gone',
  'io',
] as const;

const checks: [string, readonly string[], object][] = [
  [
    'LibraryView',
    VIEW_KEYS satisfies AllKeys<LibraryView, typeof VIEW_KEYS>,
    sample.library_view,
  ],
  [
    'EntryView',
    ENTRY_KEYS satisfies AllKeys<EntryView, typeof ENTRY_KEYS>,
    sample.library_view.entries[0]!,
  ],
  [
    'SystemView',
    SYSTEM_KEYS satisfies AllKeys<SystemView, typeof SYSTEM_KEYS>,
    sample.library_view.systems[0]!,
  ],
  [
    'ProfileView',
    PROFILE_KEYS satisfies AllKeys<ProfileView, typeof PROFILE_KEYS>,
    sample.library_view.profile,
  ],
  [
    'ScanProgress',
    PROGRESS_KEYS satisfies AllKeys<ScanProgress, typeof PROGRESS_KEYS>,
    sample.scan_progress,
  ],
  [
    'RecentLibrary',
    RECENT_KEYS satisfies AllKeys<RecentLibrary, typeof RECENT_KEYS>,
    sample.recent,
  ],
  [
    'IdentityView',
    IDENTITY_KEYS satisfies AllKeys<IdentityView, typeof IDENTITY_KEYS>,
    sample.identity,
  ],
  [
    'JobView',
    JOB_KEYS satisfies AllKeys<JobView, typeof JOB_KEYS>,
    sample.job_view,
  ],
  [
    'JobItemView',
    ITEM_KEYS satisfies AllKeys<JobItemView, typeof ITEM_KEYS>,
    sample.job_item,
  ],
  [
    'CandidateView',
    CANDIDATE_KEYS satisfies AllKeys<CandidateView, typeof CANDIDATE_KEYS>,
    sample.job_item.candidates[0]!,
  ],
  [
    'JobProgressEvent',
    JOB_PROGRESS_KEYS satisfies AllKeys<
      JobProgressEvent,
      typeof JOB_PROGRESS_KEYS
    >,
    sample.job_progress,
  ],
  [
    'EntriesUpdated',
    UPDATED_KEYS satisfies AllKeys<EntriesUpdated, typeof UPDATED_KEYS>,
    sample.entries_updated,
  ],
  [
    'UndoView',
    UNDO_KEYS satisfies AllKeys<UndoView, typeof UNDO_KEYS>,
    sample.undo,
  ],
  [
    'JobHistoryItem',
    HISTORY_KEYS satisfies AllKeys<JobHistoryItem, typeof HISTORY_KEYS>,
    sample.history_item,
  ],
  [
    'UpdateOffer',
    UPDATE_OFFER_KEYS satisfies AllKeys<UpdateOffer, typeof UPDATE_OFFER_KEYS>,
    sample.update_check_available.offer,
  ],
  [
    'UpdateProgress',
    UPDATE_PROGRESS_KEYS satisfies AllKeys<
      UpdateProgress,
      typeof UPDATE_PROGRESS_KEYS
    >,
    sample.update_progress,
  ],
  [
    'UpdateInstall',
    UPDATE_INSTALL_KEYS satisfies AllKeys<
      UpdateInstall,
      typeof UPDATE_INSTALL_KEYS
    >,
    sample.update_install,
  ],
  [
    'Volume',
    VOLUME_KEYS satisfies AllKeys<Volume, typeof VOLUME_KEYS>,
    sample.volume,
  ],
  [
    'FolderProbe',
    PROBE_KEYS satisfies AllKeys<FolderProbe, typeof PROBE_KEYS>,
    sample.folder_probe,
  ],
  [
    'OrganizePlanView',
    PLAN_VIEW_KEYS satisfies AllKeys<OrganizePlanView, typeof PLAN_VIEW_KEYS>,
    sample.organize_plan_view,
  ],
  [
    'OrganizePlan',
    PLAN_KEYS satisfies AllKeys<OrganizePlan, typeof PLAN_KEYS>,
    sample.organize_plan_view.plan,
  ],
  [
    'OrganizeItem',
    ORGANIZE_ITEM_KEYS satisfies AllKeys<
      OrganizeItem,
      typeof ORGANIZE_ITEM_KEYS
    >,
    sample.organize_plan_view.plan.items[0]!,
  ],
  [
    'OrganizeCounts',
    COUNTS_KEYS satisfies AllKeys<OrganizeCounts, typeof COUNTS_KEYS>,
    sample.organize_plan_view.plan.counts,
  ],
  [
    'OrganizeProgressEvent',
    ORGANIZE_PROGRESS_KEYS satisfies AllKeys<
      OrganizeProgressEvent,
      typeof ORGANIZE_PROGRESS_KEYS
    >,
    sample.organize_progress,
  ],
  [
    'OrganizeJobView',
    ORGANIZE_JOB_KEYS satisfies AllKeys<
      OrganizeJobView,
      typeof ORGANIZE_JOB_KEYS
    >,
    sample.organize_job,
  ],
  [
    'OrganizeReport',
    ORGANIZE_REPORT_KEYS satisfies AllKeys<
      OrganizeReport,
      typeof ORGANIZE_REPORT_KEYS
    >,
    sample.organize_job.report,
  ],
  [
    'OrganizeUndoView',
    ORGANIZE_UNDO_KEYS satisfies AllKeys<
      OrganizeUndoView,
      typeof ORGANIZE_UNDO_KEYS
    >,
    sample.organize_undo,
  ],
  [
    'OrganizeUndoReport',
    UNDO_REPORT_KEYS satisfies AllKeys<
      OrganizeUndoReport,
      typeof UNDO_REPORT_KEYS
    >,
    sample.organize_undo.report[0]!,
  ],
  [
    'OrganizeRequest',
    ORGANIZE_REQUEST_KEYS satisfies AllKeys<
      OrganizeRequest,
      typeof ORGANIZE_REQUEST_KEYS
    >,
    requests.organize_request,
  ],
  [
    'FillRequest',
    REQUEST_KEYS satisfies AllKeys<FillRequest, typeof REQUEST_KEYS>,
    requests.fill_request,
  ],
];

describe('wire contract', () => {
  it.each(checks)('%s keys match the Rust sample', (_name, keys, value) => {
    expect(Object.keys(value).sort()).toEqual([...keys].sort());
  });

  it('art marks and media use the Rust spellings', () => {
    expect(sample.library_view.entries[0]!.art).toEqual({
      cover: 'present',
      screenshot: 'unsupported',
      title: 'unsupported',
    });
    expect(sample.library_view.profile.media).toEqual(['cover']);
  });

  it('job payloads use the Rust spellings', () => {
    expect(sample.job_view.paused.kind).toBe('rate-limit');
    expect(sample.job_item.decision).toEqual({ choice: 'pick', candidate: 0 });
    expect(sample.job_item.outcome.error).toBe('write-denied');
    expect(sample.item_error_kinds).toEqual([
      ...(ITEM_ERROR_KINDS satisfies AllMembers<
        ItemErrorKind,
        typeof ITEM_ERROR_KINDS
      >),
    ]);
    const decisions = requests.decisions as DecisionIn[];
    expect(decisions[1]).toEqual({ index: 1, choice: 'pick', candidate: 2 });
    expect(requests.item_filters).toEqual(['all', 'auto', 'review', 'failed']);
  });
});

describe('update contract', () => {
  it('update checks use the Rust tags and keys', () => {
    const checks: UpdateCheck[] = [
      sample.update_check_available as UpdateCheck,
      sample.update_check_up_to_date as UpdateCheck,
      sample.update_check_skipped as UpdateCheck,
    ];
    expect(checks.map((c) => c.kind)).toEqual([
      'available',
      'up-to-date',
      'skipped',
    ]);
    expect(Object.keys(sample.update_check_available).sort()).toEqual([
      'current',
      'kind',
      'offer',
    ]);
    expect(Object.keys(sample.update_check_up_to_date).sort()).toEqual([
      'current',
      'kind',
      'latest',
    ]);
    expect(Object.keys(sample.update_check_skipped)).toEqual(['kind']);
  });
});

describe('settings contract', () => {
  it('the UI defaults and ranges are the Rust ones', () => {
    expect(DEFAULT_SETTINGS).toEqual(sample.settings);
    expect(RANGES).toEqual(sample.ranges);
    expect(BUILTIN_LAYOUTS).toEqual(
      sample.builtin_profiles.map((p) => p.profile.id),
    );
  });
  it('error and status samples have the TS shapes', () => {
    const e: FieldError = sample.field_error as FieldError;
    expect(Object.keys(e).sort()).toEqual(['code', 'field', 'max', 'min']);
    const st: CredentialStatus = sample.credential_status as CredentialStatus;
    expect(Object.keys(st.set).sort()).toEqual([...CREDENTIAL_KEYS].sort());
    const info: ProfileInfo = sample.profile_info as ProfileInfo;
    expect(Object.keys(info).sort()).toEqual([
      'builtin',
      'mirror_subdirs',
      'profile',
    ]);
    expect(Object.keys(sample.settings_view_meta).sort()).toEqual([
      'backup',
      'fixed',
      'notice',
    ]);
    expect(Object.keys(sample.profile_preview).sort()).toEqual([
      'errors',
      'paths',
    ]);
  });
  it('source info, key checks and pick reasons match Rust', () => {
    const info: SourceInfo = sample.source_info as SourceInfo;
    expect(Object.keys(info).sort()).toEqual(['built', 'id', 'needs', 'ready']);
    expect(sample.key_checks).toEqual([...KEY_CHECKS]);
    const reasons = sample.pick_reasons as PickReason[];
    expect(reasons.map((r) => r.why)).toEqual([
      'only',
      'consensus',
      'source-order',
      'region',
      'user',
    ]);
  });
  it('the mock built-in profiles are the Rust ones', () => {
    expect(MOCK_BUILTIN_PROFILES).toEqual(sample.builtin_profiles);
  });
  it('requests use the Rust spellings', () => {
    const patch: SettingsPatch = requests.settings_patch as SettingsPatch;
    expect(patch.safety?.backup_dir).toBeNull();
    expect(requests.credential_keys).toEqual([...CREDENTIAL_KEYS]);
  });
});

describe('organise contract', () => {
  const plan = sample.organize_plan_view.plan as OrganizePlan;

  it('plan items use the Rust tags', () => {
    expect(plan.items.map((i) => i.action)).toEqual(['auto', 'review', 'left']);
    const left = plan.items[2]!;
    const reason: LeftReason | null =
      left.action === 'left' ? left.reason : null;
    expect(reason).toEqual({ kind: 'partial-download' });
    expect(plan.items[0]!.rename).toEqual({
      kind: 'collision',
      name: 'Tetris Worlds (USA) (2).zip',
    });
    expect(plan.items[0]!.why[0]!.kind).toBe('hash');
    expect(plan.items[1]!.alternatives).toEqual([['gbc', 0.25]]);
    expect(plan.items[1]!.notes).toEqual([
      { kind: 'no-folder-on-device', folder: 'GB' },
    ]);
    expect(plan.warnings).toEqual([
      { kind: 'catalogue-missing', system: 'gb' },
    ]);
  });

  it('job views, outcomes and undo use the Rust spellings', () => {
    const job = sample.organize_job as OrganizeJobView;
    const outcomes: OrganizeItemOutcome[] = job.report!.items;
    expect(outcomes.map((o) => o.status)).toEqual(['moved', 'skipped']);
    expect(outcomes[1]).toMatchObject({ reason: 'not-decided' });
    expect(job.report).toMatchObject({
      paused: 'volume',
      pause_reason: 'disk-full',
    });
    expect((sample.organize_job_applying as OrganizeJobView).progress).toEqual(
      sample.organize_progress,
    );
    const failed = sample.organize_job_failed as OrganizeJobView;
    expect(failed.error).toEqual({ code: 'no-space', detail: '8473027/1000' });
    const undo = sample.organize_undo as OrganizeUndoView;
    expect(undo.later_jobs[0]!.kind).toBe('fill');
  });

  it('requests, decisions and organise errors match Rust', () => {
    const bare: OrganizeRequest = requests.organize_request_bare;
    expect(bare).toEqual({ source: '/Users/me/ROMs-Downloaded' });
    const decisions = requests.organize_decisions as OrganizeDecision[];
    expect(decisions.map((d) => d.choice)).toEqual([
      'accept',
      'assign',
      'leave',
    ]);
    for (const e of sample.organize_errors) {
      expect(ERROR_CODES).toContain(e.code);
    }
    expect(ERROR_CODES).toContain('cancelled');
  });
});
