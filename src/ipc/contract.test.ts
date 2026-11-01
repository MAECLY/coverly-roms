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
  type DedupeJobView,
  type DedupeOutcomeView,
  type DedupePlanView,
  type DedupePlanViewWire,
  type DedupeProgressEvent,
  type DedupeReportView,
  type DedupeRequest,
  type DupGroupView,
  type ArtSeenView,
  type DumpFlagsView,
  MOD_CLASSES,
  type MemberFactsView,
  type MemberView,
  type TotalsView,
  type UnitRefView,
  type CredentialStatus,
  type FieldError,
  type ProfileInfo,
  type SettingsPatch,
  type DecisionIn,
  type DiscId,
  type ExtractEntry,
  type FoundTool,
  type ItemNote,
  type IdSource,
  type NotARom,
  type OrganizeItemKind,
  type TitleIdRef,
  type Transfer,
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
  type OrganizeJobViewWire,
  type OrganizePlan,
  type OrganizePlanWire,
  type OrganizeReportWire,
  type OrganizeCountsWire,
  type OrganizePlanView,
  type OrganizeProgressEvent,
  type OrganizeReport,
  type OrganizeRequest,
  type OrganizeUndoView,
  type OrganizeUndoViewWire,
  type OrganizeUndoReport,
  type ProfileView,
  type RecentLibrary,
  type ScanProgress,
  type SystemInfo,
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
  'duplicates',
  'set_aside_trash',
  'updates',
] as const;
/** Keys Rust leaves out of a plan with no duplicates step and no updates. */
const PLAN_OMITTED = ['duplicates', 'set_aside_trash', 'updates'] as const;
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
/** `title_id` is left out of an item with no console title id. */
const CONSOLE_ITEM_KEYS = [...ORGANIZE_ITEM_KEYS, 'title_id'] as const;
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
  'set_aside',
  'in_library',
] as const;
const ORGANIZE_PROGRESS_KEYS = ['plan_id', 'phase', 'done', 'total'] as const;
const EXTRACT_PROGRESS_KEYS = [
  ...ORGANIZE_PROGRESS_KEYS,
  'item',
  'bytes_done',
  'bytes_total',
  'entries_done',
  'entries_total',
] as const;
const EXTRACT_TRANSFER_KEYS = [
  'kind',
  'entries',
  'bytes',
  'archive_after',
  'source',
  'sheet_check',
  'm3u',
] as const;
const EXTRACT_ENTRY_KEYS = [
  'index',
  'name',
  'size',
  'crc32',
  'target',
] as const;
const DISC_KEYS = ['serials', 'disc_no', 'volume', 'source'] as const;
const FOUND_TOOL_KEYS = ['tool', 'version', 'path'] as const;
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
  'extracted',
  'skipped',
  'failed',
  'set_aside',
  'items',
  'paused',
  'pause_reason',
  'resumable_items',
] as const;
const DEDUPE_PLAN_KEYS = [
  'plan_id',
  'version',
  'root',
  'roms_dir',
  'quarantine',
  'same_volume',
  'removable',
  'profile_id',
  'groups',
  'unchanged',
  'not_roms',
  'unreadable',
  'no_catalogue',
  'chd_unread',
  'off_card_saves',
  'totals',
  'trash',
  'trash_available',
] as const;
const DUP_GROUP_KEYS = [
  'id',
  'system',
  'title',
  'keeper',
  'reason',
  'runner_up',
  'members',
  'action',
  'tie',
  'choice_dropped',
] as const;
const MEMBER_KEYS = [
  'id',
  'unit',
  'relation',
  'size',
  'travels',
  'stays',
  'dest',
  'catalogue_name',
  'serials',
  'confidence',
] as const;
const MEMBER_FACTS_KEYS = [
  'tags_from',
  'regions',
  'languages',
  'revision',
  'class',
  'flags',
  'flags_from',
  'modification',
  'modification_from',
  'other',
  'saves',
  'played',
  'art',
] as const;
const DUMP_FLAGS_KEYS = ['verified', 'bad', 'overdump', 'alternate'] as const;
const ART_SEEN_KEYS = ['media', 'path', 'size'] as const;
const UNIT_KEYS = [
  'primary',
  'files',
  'kind',
  'mtime_ns',
  'in_library',
] as const;
const TOTALS_KEYS = [
  'groups',
  'extras',
  'bytes',
  'conflicts',
  'by_system',
] as const;
const DEDUPE_PROGRESS_KEYS = ['plan_id', 'phase', 'done', 'total'] as const;
const DEDUPE_JOB_KEYS = [
  'job',
  'plan_id',
  'state',
  'progress',
  'report',
  'root',
  'quarantine',
  'trash',
  'error',
] as const;
const DEDUPE_REPORT_KEYS = [
  'job',
  'outcomes',
  'bytes_out',
  'art_left',
  'paused',
  'resumable',
  'notes',
] as const;
const DEDUPE_OUTCOME_KEYS = [
  'group',
  'member',
  'status',
  'to',
  'files',
] as const;
const ORGANIZE_UNDO_KEYS = ['report', 'later_jobs'] as const;
const UNDO_REPORT_KEYS = [
  'job',
  'removed',
  'restored',
  'skipped_modified',
  'skipped_occupied',
  'dirs_removed',
  'archive_lost',
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
  'duplicates',
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
    {
      ...sample.organize_plan_view.plan,
      duplicates: [],
      set_aside_trash: true,
      updates: sample.organize_plan_updates,
    },
  ],
  [
    'OrganizeItem',
    ORGANIZE_ITEM_KEYS satisfies AllKeys<
      Omit<OrganizeItem, 'title_id'>,
      typeof ORGANIZE_ITEM_KEYS
    >,
    sample.organize_plan_view.plan.items[0]!,
  ],
  [
    'OrganizeItem (game folder)',
    CONSOLE_ITEM_KEYS satisfies AllKeys<OrganizeItem, typeof CONSOLE_ITEM_KEYS>,
    sample.organize_folder_item,
  ],
  [
    'OrganizeCounts',
    COUNTS_KEYS satisfies AllKeys<OrganizeCounts, typeof COUNTS_KEYS>,
    sample.organize_counts_set_aside,
  ],
  [
    'OrganizeItem (set aside)',
    [...ORGANIZE_ITEM_KEYS, 'keeper', 'relation', 'reason', 'dest'],
    sample.organize_set_aside_item,
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
    'OrganizeProgressEvent (extract)',
    EXTRACT_PROGRESS_KEYS satisfies AllKeys<
      Extract<OrganizeProgressEvent, { phase: 'extract' }>,
      typeof EXTRACT_PROGRESS_KEYS
    >,
    sample.organize_progress_extract,
  ],
  ['OrganizeItem (extract)', ORGANIZE_ITEM_KEYS, sample.organize_extract_item],
  [
    'Transfer (extract)',
    EXTRACT_TRANSFER_KEYS satisfies AllKeys<
      Extract<Transfer, { kind: 'extract' }>,
      typeof EXTRACT_TRANSFER_KEYS
    >,
    sample.organize_extract_item.transfer,
  ],
  [
    'ExtractEntry',
    EXTRACT_ENTRY_KEYS satisfies AllKeys<
      ExtractEntry,
      typeof EXTRACT_ENTRY_KEYS
    >,
    sample.organize_extract_item.transfer.entries[0]!,
  ],
  [
    'DiscId',
    DISC_KEYS satisfies AllKeys<DiscId, typeof DISC_KEYS>,
    sample.organize_extract_item.disc,
  ],
  [
    'FoundTool',
    FOUND_TOOL_KEYS satisfies AllKeys<FoundTool, typeof FOUND_TOOL_KEYS>,
    sample.settings_external_tools_found[0]!,
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
    {
      ...sample.organize_report_extracted,
      ...sample.organize_report_set_aside,
    },
  ],
  [
    'DedupePlanView',
    DEDUPE_PLAN_KEYS satisfies AllKeys<DedupePlanView, typeof DEDUPE_PLAN_KEYS>,
    { ...sample.dedupe_plan_view, trash: false },
  ],
  [
    'DupGroupView',
    DUP_GROUP_KEYS satisfies AllKeys<DupGroupView, typeof DUP_GROUP_KEYS>,
    sample.dedupe_plan_view.groups[0]!,
  ],
  [
    'MemberView',
    // `gamelist`, `art_leaves` and `facts` are left out when there are none.
    MEMBER_KEYS satisfies AllKeys<
      Omit<MemberView, 'gamelist' | 'art_leaves' | 'facts'>,
      typeof MEMBER_KEYS
    >,
    sample.dedupe_plan_view.groups[0]!.members[1]!,
  ],
  [
    'MemberFactsView',
    // The sample keeper carries every fact.
    MEMBER_FACTS_KEYS satisfies AllKeys<
      MemberFactsView,
      typeof MEMBER_FACTS_KEYS
    >,
    sample.dedupe_plan_view.groups[0]!.members[0]!.facts!,
  ],
  [
    'DumpFlagsView',
    DUMP_FLAGS_KEYS satisfies AllKeys<DumpFlagsView, typeof DUMP_FLAGS_KEYS>,
    sample.dedupe_plan_view.groups[0]!.members[0]!.facts!.flags,
  ],
  [
    'ArtSeenView',
    ART_SEEN_KEYS satisfies AllKeys<ArtSeenView, typeof ART_SEEN_KEYS>,
    sample.dedupe_plan_view.groups[0]!.members[0]!.facts!.art[0]!,
  ],
  [
    'UnitRefView',
    // `mtimes_ns` is left out for a unit of one file.
    UNIT_KEYS satisfies AllKeys<
      Omit<UnitRefView, 'mtimes_ns'>,
      typeof UNIT_KEYS
    >,
    sample.dedupe_plan_view.groups[0]!.members[0]!.unit,
  ],
  [
    'TotalsView',
    TOTALS_KEYS satisfies AllKeys<TotalsView, typeof TOTALS_KEYS>,
    sample.dedupe_plan_view.totals,
  ],
  [
    'DedupeProgressEvent',
    DEDUPE_PROGRESS_KEYS satisfies AllKeys<
      DedupeProgressEvent,
      typeof DEDUPE_PROGRESS_KEYS
    >,
    sample.dedupe_progress,
  ],
  [
    'DedupeJobView',
    DEDUPE_JOB_KEYS satisfies AllKeys<DedupeJobView, typeof DEDUPE_JOB_KEYS>,
    sample.dedupe_job,
  ],
  [
    'DedupeReportView',
    DEDUPE_REPORT_KEYS satisfies AllKeys<
      DedupeReportView,
      typeof DEDUPE_REPORT_KEYS
    >,
    sample.dedupe_job.report,
  ],
  [
    'DedupeOutcomeView (quarantined)',
    DEDUPE_OUTCOME_KEYS satisfies AllKeys<
      Extract<DedupeOutcomeView, { status: 'quarantined' }>,
      typeof DEDUPE_OUTCOME_KEYS
    >,
    sample.dedupe_job.report.outcomes[0]!,
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
    sample.organize_undo_archive_lost,
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
  const plan = sample.organize_plan_view.plan as OrganizePlanWire;

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
    const job = sample.organize_job as OrganizeJobViewWire;
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
    const undo = sample.organize_undo as OrganizeUndoViewWire;
    expect(undo.later_jobs[0]!.kind).toBe('fill');
  });

  it('Rust leaves zero extractions and no lost archive out of the JSON', () => {
    const omitted = (keys: readonly string[], ...out: string[]) =>
      keys.filter((k) => !out.includes(k)).sort();
    expect(Object.keys(sample.organize_job.report).sort()).toEqual(
      omitted(ORGANIZE_REPORT_KEYS, 'extracted', 'set_aside'),
    );
    expect(Object.keys(sample.organize_undo.report[0]!).sort()).toEqual(
      omitted(UNDO_REPORT_KEYS, 'archive_lost'),
    );
    expect(Object.keys(sample.organize_plan_view.plan).sort()).toEqual(
      omitted(PLAN_KEYS, ...PLAN_OMITTED),
    );
    expect(Object.keys(sample.organize_plan_view.plan.counts).sort()).toEqual(
      omitted(COUNTS_KEYS, 'set_aside', 'in_library'),
    );
  });

  it('extract items, disc ids and progress use the Rust spellings', () => {
    const item = sample.organize_extract_item as OrganizeItem;
    expect(item.kind).toBe('archive');
    expect(item.transfer).toMatchObject({
      kind: 'extract',
      bytes: 405_749_493,
      archive_after: 'trash',
      source: { kind: 'builtin' },
      sheet_check: 'read',
      m3u: null,
    });
    const notes: ItemNote[] = item.notes;
    expect(notes).toEqual([
      { kind: 'container-mismatch', ext: 'zip', actual: '7z' },
    ]);
    const disc: DiscId | null = item.disc;
    expect(disc).toEqual({
      serials: ['SLUS-00862'],
      disc_no: null,
      volume: null,
      source: 'system-cnf',
    });
    const progress = sample.organize_progress_extract as OrganizeProgressEvent;
    expect(progress.phase).toBe('extract');
    if (progress.phase === 'extract') {
      expect([
        progress.item,
        progress.bytes_done,
        progress.bytes_total,
        progress.entries_done,
        progress.entries_total,
      ]).toEqual([3, 104_857_600, 405_749_493, 4, 12]);
    }
    const report = sample.organize_report_extracted as OrganizeReportWire;
    expect(report.extracted).toBe(1);
    expect(report.items.map((o) => o.status)).toEqual(['extracted-kept']);
    const undo = sample.organize_undo_archive_lost as OrganizeUndoReport;
    expect(undo.archive_lost).toEqual([
      '/Users/me/ROMs-Downloaded/Next Tetris, The.zip',
    ]);
    const tools: FoundTool[] =
      sample.settings_external_tools_found as FoundTool[];
    expect(tools.map((t) => t.tool)).toEqual(['seven-zip']);
  });

  it('the new left reasons and notes type-check with the Rust fields', () => {
    const reasons: LeftReason[] = [
      { kind: 'unsupported-archive', format: 'rar', need: 'unrar' },
      { kind: 'encrypted-archive' },
      { kind: 'archive-needs-memory', mib: 1536 },
      { kind: 'archive-too-big', entries: 20_000, bytes: null },
      { kind: 'multi-volume-archive', parts: 2 },
      { kind: 'unsafe-archive-entry', name: '../x.bin', why: 'dot-dot' },
      { kind: 'set-unsafe-name', name: 'a:b.bin' },
      { kind: 'path-too-long' },
      { kind: 'nested-archive' },
      { kind: 'repack-arcade' },
      { kind: 'folder-full', folder: 'PS' },
      { kind: 'unsafe-target' },
    ];
    const notes: ItemNote[] = [
      { kind: 'twin-sheets', kept: ['a.cue', 'a.gdi'] },
      { kind: 'no-stored-checksum' },
      { kind: 'archive-not-fully-read' },
      { kind: 'archive-kept-leftovers', count: 2, examples: ['readme.txt'] },
      { kind: 'sheet-not-read-yet' },
      { kind: 'multi-rom-extract', count: 3 },
    ];
    expect(reasons).toHaveLength(12);
    expect(notes).toHaveLength(6);
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
    for (const e of [...sample.organize_errors, ...sample.dedupe_errors]) {
      expect(ERROR_CODES).toContain(e.code);
    }
    expect(ERROR_CODES).toContain('cancelled');
  });
});

describe('duplicates contract', () => {
  it('contract_round_trip', () => {
    // Requests: what the webview sends reads in Rust (rust_reads_the_shared_dedupe_requests).
    const full: DedupeRequest = requests.dedupe_request as DedupeRequest;
    expect(full.quarantine).toEqual({
      kind: 'folder',
      path: '/Users/me/Duplicados',
    });
    const bare: DedupeRequest = requests.dedupe_request_bare;
    expect(bare).toEqual({});
    const trash = requests.dedupe_request_trash as DedupeRequest;
    expect(trash.quarantine).toEqual({ kind: 'trash' });

    // Views: Rust leaves `trash` out of a plan made for the quarantine.
    const wire = sample.dedupe_plan_view as unknown as DedupePlanViewWire;
    expect('trash' in wire).toBe(false);
    expect(wire.trash_available).toBe(false);
    expect(wire.groups[0]!.reason).toEqual({
      kind: 'catalogue-name',
      name: 'Tetris Worlds (USA)',
    });
    const progress: DedupeProgressEvent[] = [
      sample.dedupe_progress as DedupeProgressEvent,
      sample.dedupe_progress_apply as DedupeProgressEvent,
    ];
    expect(progress.map((p) => p.phase)).toEqual(['fingerprints', 'apply']);

    const job = sample.dedupe_job as DedupeJobView;
    expect(job.report!.outcomes.map((o) => o.status)).toEqual([
      'quarantined',
      'skipped',
    ]);
    const skipped = job.report!.outcomes[1]!;
    expect(skipped.status === 'skipped' && skipped.reason).toEqual({
      kind: 'conflict',
    });
    expect((sample.dedupe_job_applying as DedupeJobView).progress).toEqual(
      sample.dedupe_progress_apply,
    );
    expect((sample.dedupe_job_failed as DedupeJobView).error).toEqual({
      code: 'no-space',
      detail: '4194304/1000',
    });
    for (const e of sample.dedupe_errors) expect(ERROR_CODES).toContain(e.code);

    // Facts: a member without any is written without the key.
    const [keeper, extra] = wire.groups[0]!.members;
    expect('facts' in extra!).toBe(false);
    const facts: MemberFactsView = keeper!.facts!;
    expect(facts.tags_from).toBe('catalogue');
    expect(facts.flags_from).toBe('file-name');
    expect(facts.regions).toEqual(['usa']);
    expect(facts.class).toBe('beta');
    expect(MOD_CLASSES).toContain(facts.modification);
    expect(facts.saves).toEqual({ count: 1, last_ns: 1700000050000000000 });
    expect(facts.played).toEqual({ count: 4, last: '20260102T101010' });
    expect(facts.art![0]!.media).toBe('cover');
  });

  it('organise set-aside items, in-library reasons and counts use the Rust spellings', () => {
    const item = sample.organize_set_aside_item as OrganizeItem;
    expect(item.action).toBe('set-aside');
    if (item.action === 'set-aside') {
      expect(item.keeper).toBe('ROMS/GB/Tetris (World) (Rev 1).gb');
      expect(item.relation).toBe('exact');
      expect(item.reason).toEqual({ kind: 'in-library' });
      expect(item.dest).toBe('_Duplicados/Tetris (1).gb');
    }
    const reason = sample.organize_in_library_reason as LeftReason;
    expect(reason).toEqual({
      kind: 'in-library',
      path: 'ROMS/GB/Tetris (World) (Rev 1).gb',
      relation: 'version',
    });
    const counts: OrganizeCountsWire = sample.organize_counts_set_aside;
    expect([counts.set_aside, counts.in_library]).toEqual([1, 2]);
    const report = sample.organize_report_set_aside as OrganizeReportWire;
    expect(report.items.map((o) => o.status)).toEqual([
      'quarantined',
      'trashed',
    ]);
    expect(report.set_aside).toBe(2);
    expect(report.extracted).toBeUndefined();
  });

  it('every SystemInfo names its family and a folder game names its folder', () => {
    const systems = sample.system_infos as SystemInfo[];
    expect(systems.map((s) => s.family)).toEqual([
      'console',
      'handheld',
      'computer',
      'arcade',
      'engine',
    ]);
    for (const s of systems) expect(s.family).toBeTruthy();
    const reason = sample.organize_folder_game_reason as LeftReason;
    expect(reason).toEqual({
      kind: 'folder-game',
      system: 'doom',
      folder: 'prboom',
    });
  });

  it('systems batch 2: game folders, title ids, the new notes and reasons use the Rust spellings', () => {
    const item = sample.organize_folder_item as OrganizeItem;
    const kind: OrganizeItemKind = item.kind;
    expect(kind).toBe('folder');
    expect(item.size).toBe(23_410_000_000);
    const tid: TitleIdRef = item.title_id!;
    expect(tid).toEqual({ id: 'BLUS30443', source: 'param-sfo' });
    // Every note kind of the batch, in Rust's order.
    const notes: ItemNote['kind'][] = item.notes.map((n) => n.kind);
    expect(notes).toEqual([
      'folder-suffix',
      'update',
      'dlc',
      'encrypted-content',
      'install-first',
    ]);
    expect(item.notes[0]).toEqual({
      kind: 'folder-suffix',
      name: "Demon's Souls (BLUS-30443).ps3",
      layout: 'ES-DE',
    });
    const sources: IdSource[] = sample.organize_id_sources as IdSource[];
    expect(sources).toEqual(ID_SOURCES);
    const reasons = sample.organize_batch2_reasons as LeftReason[];
    expect(reasons).toEqual([
      { kind: 'key-material' },
      { kind: 'emulator-data', folder: 'ux0' },
      { kind: 'not-a-rom', what: 'installer' },
      { kind: 'not-a-rom', what: 'model3d' },
      { kind: 'not-a-rom', what: 'pc-game-data' },
    ]);
    const what: NotARom[] = reasons.flatMap((r) =>
      r.kind === 'not-a-rom' ? [r.what] : [],
    );
    expect(what).toEqual(['installer', 'model3d', 'pc-game-data']);
    // Round trip through the wire form.
    expect(JSON.parse(JSON.stringify(item))).toEqual(
      sample.organize_folder_item,
    );
    expect(sample.organize_plan_updates).toBe(2);
    // An older plan has no `updates`; it reads as none.
    expect(
      (sample.organize_plan_view.plan as OrganizePlanWire).updates,
    ).toBeUndefined();
  });
});

/** Every `IdSource`, in Rust's declaration order. */
const ID_SOURCES = [
  'param-sfo',
  'meta-xml',
  'app-xml',
  'default-xbe',
  'default-xex',
  'cnmt-xml',
  'ticket-name',
  'file-name',
  'header',
] as const satisfies readonly IdSource[];
// Compile-time: the list names every member.
const _allSources: Exclude<IdSource, (typeof ID_SOURCES)[number]> extends never
  ? true
  : never = true;
void _allSources;
