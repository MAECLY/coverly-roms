import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it, vi } from 'vitest';

import requests from './contract.requests.json';
import sample from './contract.sample.json';
import { BackendError } from './errors';
import type { DedupeRequest, OrganizeDecision, OrganizeRequest } from './types';

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => dialog);

const events = vi.hoisted(() => ({
  handlers: new Map<string, (e: { payload: unknown }) => void>(),
}));
// Keep the module's other exports: `@tauri-apps/api/webview` imports them.
vi.mock('@tauri-apps/api/event', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tauri-apps/api/event')>()),
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    events.handlers.set(name, cb);
    return () => events.handlers.delete(name);
  }),
}));

import {
  createTauriBackend,
  DEDUPE_CHANGED_EVENT,
  DEDUPE_PROGRESS_EVENT,
  ENTRIES_UPDATED_EVENT,
  JOB_CHANGED_EVENT,
  ORGANIZE_CHANGED_EVENT,
  ORGANIZE_PROGRESS_EVENT,
  SCAN_PROGRESS_EVENT,
  UPDATE_PROGRESS_EVENT,
} from './tauriBackend';

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

describe('Tauri backend', () => {
  afterEach(() => {
    clearMocks();
    dialog.open.mockReset();
  });

  it('opens a library with camelCase arguments', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return sample.library_view;
    });
    const view = await createTauriBackend().openLibrary('/Volumes/SD');
    expect(view.entries).toHaveLength(1);
    expect(calls).toEqual([
      ['open_library', { path: '/Volumes/SD', profileId: null }],
    ]);
  });

  it('passes entry ids and media to read_art and sniffs the bytes', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return PNG.buffer;
    });
    const art = await createTauriBackend().readArt(3, 9, 'cover');
    expect(art.mime).toBe('image/png');
    expect(calls).toEqual([
      ['read_art', { generation: 3, entryId: 9, media: 'cover' }],
    ]);
  });

  it('refuses bytes that are not an image', async () => {
    mockIPC(() => new TextEncoder().encode('<svg/>').buffer);
    await expect(
      createTauriBackend().readArt(1, 0, 'cover'),
    ).rejects.toMatchObject({ code: 'art-unreadable' });
  });

  it('maps command errors to BackendError', async () => {
    mockIPC(() => Promise.reject({ code: 'not-found', detail: '/x' }));
    const err = await createTauriBackend()
      .openLibrary('/x')
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(BackendError);
    expect(err).toMatchObject({ code: 'not-found', detail: '/x' });
  });

  it('sends ignored folders as ignoredDirs', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return { ignored_dirs: ['bios'] };
    });
    await createTauriBackend().setLibrarySettings({ ignored_dirs: ['bios'] });
    expect(calls).toEqual([
      ['set_library_settings', { ignoredDirs: ['bios'] }],
    ]);
  });

  it('forwards scan progress events', async () => {
    const seen: unknown[] = [];
    const off = await createTauriBackend().onScanProgress((p) => seen.push(p));
    events.handlers.get(SCAN_PROGRESS_EVENT)!({
      payload: sample.scan_progress,
    });
    expect(seen).toEqual([sample.scan_progress]);
    off();
    expect(events.handlers.has(SCAN_PROGRESS_EVENT)).toBe(false);
  });

  it('picks a single folder with the dialog', async () => {
    dialog.open.mockResolvedValueOnce('/picked');
    await expect(createTauriBackend().pickFolder()).resolves.toBe('/picked');
    expect(dialog.open).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
    });
    dialog.open.mockResolvedValueOnce(null);
    await expect(createTauriBackend().pickFolder()).resolves.toBeNull();
  });

  it('starts a fill with the request object and forwards job events', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return sample.job_view;
    });
    const b = createTauriBackend();
    const request = {
      entry_ids: null,
      media: ['cover'] as const,
      strategy: 'rom-region' as const,
      regions: ['rom', 'usa'] as const,
      overwrite: 'never' as const,
    };
    await b.startFill(3, {
      ...request,
      media: [...request.media],
      regions: [...request.regions],
    });
    await b.decideFill(4, [{ index: 0, choice: 'accept' }]);
    await b.fillJobItems(4, 'review');
    expect(calls).toEqual([
      [
        'start_fill',
        {
          generation: 3,
          request: { ...request, media: ['cover'], regions: ['rom', 'usa'] },
        },
      ],
      ['decide_fill', { job: 4, decisions: [{ index: 0, choice: 'accept' }] }],
      ['fill_job_items', { job: 4, filter: 'review' }],
    ]);
    const seen: unknown[] = [];
    await b.onJobChanged((v) => seen.push(v));
    await b.onEntriesUpdated((u) => seen.push(u));
    events.handlers.get(JOB_CHANGED_EVENT)!({ payload: sample.job_view });
    events.handlers.get(ENTRIES_UPDATED_EVENT)!({
      payload: sample.entries_updated,
    });
    expect(seen).toEqual([sample.job_view, sample.entries_updated]);
  });

  it('sniffs candidate art and passes drawer arguments in camelCase', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return cmd.endsWith('_art') ? PNG.buffer : null;
    });
    const b = createTauriBackend();
    expect((await b.candidateArt(4, 2, 1)).mime).toBe('image/png');
    expect((await b.entryCandidateArt(3, 9, 'cover', 0)).mime).toBe(
      'image/png',
    );
    await expect(b.setEntryArtFromFile(3, 9, 'cover')).resolves.toBeNull();
    expect(calls).toEqual([
      ['candidate_art', { job: 4, index: 2, candidate: 1 }],
      [
        'entry_candidate_art',
        { generation: 3, entryId: 9, media: 'cover', candidate: 0 },
      ],
      [
        'set_entry_art_from_file',
        { generation: 3, entryId: 9, media: 'cover' },
      ],
    ]);
  });

  it('asks Rust for source info and key checks with camelCase args', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return cmd === 'sources_info' ? [sample.source_info] : 'ok';
    });
    const b = createTauriBackend();
    expect(await b.sourcesInfo()).toEqual([sample.source_info]);
    expect(await b.testSourceKey('steamgriddb')).toBe('ok');
    expect(calls).toEqual([
      ['sources_info', {}],
      ['test_source_key', { source: 'steamgriddb' }],
    ]);
  });

  it('lists the known systems', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return [{ id: 'gba', name: 'Game Boy Advance' }];
    });
    const b = createTauriBackend();
    expect(await b.listSystems()).toEqual([
      { id: 'gba', name: 'Game Boy Advance' },
    ]);
    expect(calls).toEqual([['list_systems', {}]]);
  });

  it('sends the settings, profile, credential and search commands', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === 'entry_search_art') return PNG.buffer;
      if (cmd === 'cache_usage' || cmd === 'clear_cache') return 0;
      return null;
    });
    const b = createTauriBackend();
    await b.getSettings();
    await b.updateSettings({ network: { timeout_secs: 60 } });
    await b.resetSettings();
    await b.openConfigFolder();
    await b.openLogsFolder();
    await b.openLink('repo');
    await b.cacheUsage();
    await b.clearCache();
    await b.listProfiles();
    const profile = sample.profile_info.profile as never;
    await b.saveProfile(profile, null);
    await b.deleteProfile('mine');
    await b.previewProfile(profile, {
      system_folder: 'GBA',
      rel_dir: '',
      file_name: 'a.gba',
    });
    await b.credentialStatus();
    await b.setCredential('screenscraper-user', 'me');
    await b.clearCredential('screenscraper-user');
    await b.searchEntryArt(2, 7, 'cover', 'zelda');
    await b.entrySearchArt(2, 7, 'cover', 1);
    await b.setEntryArtFromSearch(2, 7, 'cover', 1);
    await b.clearIdentityOverride(2, 7);
    expect(calls).toEqual([
      ['get_settings', {}],
      ['update_settings', { patch: { network: { timeout_secs: 60 } } }],
      ['reset_settings', {}],
      ['open_config_folder', {}],
      ['open_logs_folder', {}],
      ['open_link', { link: 'repo' }],
      ['cache_usage', {}],
      ['clear_cache', {}],
      ['list_profiles', {}],
      ['save_profile', { profile, replaces: null }],
      ['delete_profile', { id: 'mine' }],
      [
        'preview_profile',
        {
          profile,
          example: { system_folder: 'GBA', rel_dir: '', file_name: 'a.gba' },
        },
      ],
      ['credential_status', {}],
      ['set_credential', { key: 'screenscraper-user', value: 'me' }],
      ['clear_credential', { key: 'screenscraper-user' }],
      [
        'search_entry_art',
        { generation: 2, entryId: 7, media: 'cover', query: 'zelda' },
      ],
      [
        'entry_search_art',
        { generation: 2, entryId: 7, media: 'cover', candidate: 1 },
      ],
      [
        'set_entry_art_from_search',
        { generation: 2, entryId: 7, media: 'cover', candidate: 1 },
      ],
      ['clear_identity_override', { generation: 2, entryId: 7 }],
    ]);
  });

  it('drives the updater through the Rust commands and the progress event', async () => {
    const calls: string[] = [];
    mockIPC((cmd, args) => {
      calls.push(`${cmd} ${JSON.stringify(args ?? {})}`);
      if (cmd === 'check_update') return sample.update_check_available;
      if (cmd === 'install_update') return sample.update_install;
      return null;
    });
    const b = createTauriBackend();
    await expect(b.checkForUpdates('startup')).resolves.toEqual(
      sample.update_check_available,
    );
    await b.downloadUpdate();
    await expect(b.installUpdate()).resolves.toEqual(sample.update_install);
    await b.restartApp();
    expect(calls).toEqual([
      'check_update {"trigger":"startup"}',
      'download_update {}',
      'install_update {}',
      'restart_app {}',
    ]);
    expect(UPDATE_PROGRESS_EVENT).toBe('update-progress');
    const seen: unknown[] = [];
    const off = await b.onUpdateProgress((p) => seen.push(p));
    events.handlers.get(UPDATE_PROGRESS_EVENT)!({
      payload: sample.update_progress,
    });
    expect(seen).toEqual([sample.update_progress]);
    off();
    expect(events.handlers.has(UPDATE_PROGRESS_EVENT)).toBe(false);
  });

  it('lists the connected drives', async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      return [sample.volume];
    });
    await expect(createTauriBackend().mountedVolumes()).resolves.toEqual([
      sample.volume,
    ]);
    expect(calls).toEqual(['mounted_volumes']);
  });

  it('maps an update refusal to its code', async () => {
    mockIPC(() => Promise.reject({ code: 'update-signature', detail: 'bad' }));
    await expect(createTauriBackend().downloadUpdate()).rejects.toMatchObject({
      code: 'update-signature',
      detail: 'bad',
    });
  });
  it('maps organise commands to snake_case payloads and forwards their events', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      switch (cmd) {
        case 'probe_folder':
          return sample.folder_probe;
        case 'organize_plan':
          return sample.organize_plan_view;
        case 'organize_undo':
          return sample.organize_undo;
        case 'organize_export_plan':
          return 'coverly-organize-plan.json';
        case 'organize_cancel':
        case 'organize_cancel_apply':
          return null;
        default:
          return sample.organize_job;
      }
    });
    const b = createTauriBackend();
    const decisions = requests.organize_decisions as OrganizeDecision[];
    const request = requests.organize_request as OrganizeRequest;
    await expect(b.probeFolder('/Users/me/ROMs-Downloaded')).resolves.toEqual(
      sample.folder_probe,
    );
    await b.organizePlan(request);
    await b.organizePlan({ source: '/x' }, decisions);
    await b.organizeCancel(3);
    await b.organizeApply(3, decisions);
    await b.organizeResume('j');
    await b.organizeCancelApply();
    await b.organizeUndo('j', true);
    await expect(b.organizeExportPlan(3)).resolves.toBe(
      'coverly-organize-plan.json',
    );
    expect(calls).toEqual([
      ['probe_folder', { path: '/Users/me/ROMs-Downloaded' }],
      ['organize_plan', { request, decisions: null }],
      ['organize_plan', { request: { source: '/x' }, decisions }],
      ['organize_cancel', { planId: 3 }],
      ['organize_apply', { planId: 3, decisions }],
      ['organize_resume', { job: 'j' }],
      ['organize_cancel_apply', {}],
      ['organize_undo', { job: 'j', includeLater: true }],
      ['organize_export_plan', { planId: 3 }],
    ]);

    const seen: unknown[] = [];
    const offProgress = await b.onOrganizeProgress((e) => seen.push(e));
    await b.onOrganizeChanged((v) => seen.push(v));
    events.handlers.get(ORGANIZE_PROGRESS_EVENT)!({
      payload: sample.organize_progress,
    });
    events.handlers.get(ORGANIZE_CHANGED_EVENT)!({
      payload: sample.organize_job,
    });
    expect(seen).toEqual([
      sample.organize_progress,
      {
        ...sample.organize_job,
        report: { ...sample.organize_job.report, extracted: 0, set_aside: 0 },
      },
    ]);
    offProgress();
    expect(events.handlers.has(ORGANIZE_PROGRESS_EVENT)).toBe(false);
  });

  it('old_plan_without_transfer_defaults_as_is', async () => {
    const old = structuredClone(sample.organize_plan_view) as {
      plan: { items: Record<string, unknown>[] };
    };
    for (const item of old.plan.items) {
      delete item.transfer;
      delete item.disc;
    }
    mockIPC(() => old);
    const view = await createTauriBackend().organizePlan({ source: '/x' });
    expect(view.plan.items.map((i) => i.transfer)).toEqual([
      { kind: 'as-is' },
      { kind: 'as-is' },
      { kind: 'as-is' },
    ]);
    expect(view.plan.items.map((i) => i.disc)).toEqual([null, null, null]);
  });

  it('keeps extract transfers and disc ids as Rust sent them', async () => {
    const plan = structuredClone(sample.organize_plan_view);
    plan.plan.items = [sample.organize_extract_item as never];
    mockIPC(() => plan);
    const view = await createTauriBackend().organizePlan({ source: '/x' });
    expect(view.plan.items[0]).toEqual(sample.organize_extract_item);
  });

  it('fills the counts Rust leaves out when zero or empty', async () => {
    mockIPC((cmd) =>
      cmd === 'organize_undo' ? sample.organize_undo : sample.organize_job,
    );
    const b = createTauriBackend();
    const job = await b.organizeApply(3, []);
    expect(job.report!.extracted).toBe(0);
    expect((await b.organizeResume('j')).report!.extracted).toBe(0);
    const undo = await b.organizeUndo('j', false);
    expect(undo.report.map((r) => r.archive_lost)).toEqual([[]]);

    const seen: unknown[] = [];
    await b.onOrganizeChanged((v) => seen.push(v.report?.extracted));
    events.handlers.get(ORGANIZE_CHANGED_EVENT)!({
      payload: sample.organize_job,
    });
    expect(seen).toEqual([0]);
  });

  it('forwards extract progress with its item and byte counts', async () => {
    const seen: unknown[] = [];
    await createTauriBackend().onOrganizeProgress((e) => seen.push(e));
    events.handlers.get(ORGANIZE_PROGRESS_EVENT)!({
      payload: sample.organize_progress_extract,
    });
    expect(seen).toEqual([sample.organize_progress_extract]);
  });

  it('returns the external tools the settings view lists', async () => {
    mockIPC(() => ({
      settings: sample.settings,
      notice: null,
      backup: null,
      fixed: [],
      external_tools_found: sample.settings_external_tools_found,
    }));
    const view = await createTauriBackend().getSettings();
    expect(view.external_tools_found).toEqual([
      { tool: 'seven-zip', version: '24.08', path: '/opt/homebrew/bin/7zz' },
    ]);
  });

  it('maps organise refusals to their codes', async () => {
    mockIPC(() => Promise.reject({ code: 'later-jobs', detail: 'j2' }));
    await expect(
      createTauriBackend().organizeUndo('j', false),
    ).rejects.toMatchObject({ code: 'later-jobs', detail: 'j2' });
  });

  it('tauri_adapter_uses_camelCase_args', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      switch (cmd) {
        case 'dedupe_plan':
        case 'dedupe_regroup':
        case 'dedupe_set_keeper':
        case 'dedupe_skip_group':
        case 'dedupe_skip_system':
          return sample.dedupe_plan_view;
        case 'dedupe_reset_choices':
          return 3;
        case 'dedupe_export_plan':
          return 'coverly-duplicates-plan.json';
        case 'dedupe_job':
          return null;
        case 'dedupe_cancel':
        case 'dedupe_cancel_apply':
        case 'reveal_path':
          return null;
        case 'dedupe_art':
          return PNG.buffer;
        default:
          return sample.dedupe_job;
      }
    });
    const b = createTauriBackend();
    const opts = requests.dedupe_request as DedupeRequest;
    const plan = await b.dedupePlan('/Volumes/SD', opts);
    // Rust leaves `trash` out of a quarantine plan; the adapter fills it in.
    expect(plan).toEqual({ ...sample.dedupe_plan_view, trash: false });
    await b.dedupePlan('/Volumes/SD');
    await b.dedupeCancel(5);
    await b.dedupeRegroup(5, { levels: ['exact'] });
    await b.dedupeSetKeeper(5, 4_503_599_627_370_495, 1);
    await b.dedupeSkipGroup(5, 17, true);
    await b.dedupeSkipSystem(5, 'gba', false);
    await expect(b.dedupeResetChoices('/Volumes/SD', ['gba'])).resolves.toBe(3);
    await b.dedupeResetChoices('/Volumes/SD');
    await expect(b.dedupeApply(5)).resolves.toEqual(sample.dedupe_job);
    await b.dedupeCancelApply();
    await expect(b.dedupeJob()).resolves.toBeNull();
    await b.dedupeResume('j');
    await expect(b.dedupeExportPlan(5)).resolves.toBe(
      'coverly-duplicates-plan.json',
    );
    await b.revealPath('/Volumes/SD/_Duplicados');
    const art = await b.dedupeArt(5, 17, null, 'cover');
    expect(new Uint8Array(art)).toEqual(PNG);
    await b.dedupeArt(5, 17, 1, 'screenshot');
    expect(calls).toEqual([
      ['dedupe_plan', { root: '/Volumes/SD', opts }],
      ['dedupe_plan', { root: '/Volumes/SD', opts: null }],
      ['dedupe_cancel', { planId: 5 }],
      ['dedupe_regroup', { planId: 5, opts: { levels: ['exact'] } }],
      [
        'dedupe_set_keeper',
        { planId: 5, group: 4_503_599_627_370_495, member: 1 },
      ],
      ['dedupe_skip_group', { planId: 5, group: 17, skip: true }],
      ['dedupe_skip_system', { planId: 5, system: 'gba', skip: false }],
      ['dedupe_reset_choices', { root: '/Volumes/SD', systems: ['gba'] }],
      ['dedupe_reset_choices', { root: '/Volumes/SD', systems: null }],
      ['dedupe_apply', { planId: 5 }],
      ['dedupe_cancel_apply', {}],
      ['dedupe_job', {}],
      ['dedupe_resume', { job: 'j' }],
      ['dedupe_export_plan', { planId: 5 }],
      ['reveal_path', { path: '/Volumes/SD/_Duplicados' }],
      ['dedupe_art', { planId: 5, group: 17, member: null, media: 'cover' }],
      ['dedupe_art', { planId: 5, group: 17, member: 1, media: 'screenshot' }],
    ]);

    const seen: unknown[] = [];
    const off = await b.onDedupeProgress((e) => seen.push(e));
    await b.onDedupeChanged((v) => seen.push(v));
    events.handlers.get(DEDUPE_PROGRESS_EVENT)!({
      payload: sample.dedupe_progress,
    });
    events.handlers.get(DEDUPE_CHANGED_EVENT)!({ payload: sample.dedupe_job });
    expect(seen).toEqual([sample.dedupe_progress, sample.dedupe_job]);
    off();
    expect(events.handlers.has(DEDUPE_PROGRESS_EVENT)).toBe(false);
  });

  it('fills in the organise duplicates fields Rust leaves out', async () => {
    mockIPC((cmd) =>
      cmd === 'organize_plan' ? sample.organize_plan_view : sample.organize_job,
    );
    const b = createTauriBackend();
    const view = await b.organizePlan({ source: '/x' });
    expect(view.plan.duplicates).toEqual([]);
    expect(view.plan.set_aside_trash).toBe(false);
    expect([view.plan.counts.set_aside, view.plan.counts.in_library]).toEqual([
      0, 0,
    ]);
    expect((await b.organizeApply(3, [])).report!.set_aside).toBe(0);
  });

  it('maps duplicates refusals to their codes', async () => {
    mockIPC(() => Promise.reject({ code: 'no-dedupe-plan' }));
    await expect(
      createTauriBackend().dedupeSetKeeper(9, 1, 0),
    ).rejects.toMatchObject({ code: 'no-dedupe-plan' });
  });
});
