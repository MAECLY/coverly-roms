/**
 * Mock of the settings, profiles, credentials, folders, links and cache
 * commands. Persists to `localStorage` (when usable) so a reload keeps
 * them, like the app's settings file. Whatever storage holds is untrusted:
 * garbage or wrong shapes fall back to defaults. Credential VALUES are
 * never stored: only whether each key is set.
 *
 * Sources (M6): the mock "build" has libretro and SteamGridDB, not
 * ScreenScraper (no developer credentials). "Test key" for SteamGridDB:
 * no key stored → `missing`; a key whose value started with `bad` →
 * `invalid`, with `offline` → `offline` (only that verdict is remembered,
 * in memory, never the value); otherwise `ok`. ScreenScraper answers
 * `unavailable`; libretro (no key) is a `bad-request`.
 *
 * Integraciones (integrations spec): `integrationsView` computes each
 * card's state with a TS port of the D6 precedence (test scaffolding; the
 * shipped rule is `coverly_core::integrations::state`). «Probar conexión»
 * and saving or forgetting a key change the card for the session, like
 * the app's in-memory health, and so does a fill's gather (Rust's
 * `note_troubles`: answered → `connected`, refused → `error`). URL switches, read once when the backend is
 * created (like `?mockPick=`):
 *
 * - `?mockIntegrations=empty` nothing stored, catalogue off; `saved` all
 *   three keys stored (SteamGridDB on); `error` SteamGridDB stored and
 *   refused by its last test; `quota` a developer build whose ScreenScraper
 *   account is stored and its daily quota used up; `dev` a developer build
 *   with the account stored (its test reports a quota); `nodev` the
 *   default, every official build: ScreenScraper `unavailable`.
 * - `?mockKeychain=locked|denied|unavailable` the keychain refuses: saving
 *   or clearing fails with `keychain-<kind>`, and credentialed cards are
 *   `error` with that reason (never `not-configured`).
 * - `?mockPlatform=macos|windows|linux` what `appInfo().platform` says
 *   (picks the per-OS keychain copy; default `macos`).
 */
import { CREDITS } from '@/lib/credits';
import {
  WORKING,
  integration,
  integrationForSource,
  type Integration,
} from '@/lib/integrations';
import { previewProfile, validateProfile } from '@/lib/profileTemplate';
import {
  DEFAULT_SETTINGS,
  KNOWN_SOURCES,
  cloneSettings,
  mergePatch,
  needsReopen,
  normaliseSettings,
  patchShapeErrors,
  shapeSettings,
  tolerantSettings,
  validateSettings,
} from '@/lib/settingsRules';

import type { Backend } from '../backend';
import { BackendError } from '../errors';
import { FIXTURE_SYSTEMS } from './fixture';
import { ORGANIZE_SYSTEMS } from './mockOrganize';
import {
  CREDENTIAL_KEYS,
  MEDIA,
  type CatalogStatus,
  type CredentialKey,
  type CredentialStatus,
  type FoundTool,
  type IntegrationReason,
  type IntegrationState,
  type IntegrationTestResult,
  type IntegrationView,
  type KeyCheck,
  type KeychainKind,
  type ProfileFile,
  type ProfileInfo,
  type Settings,
  type SettingsNotice,
  type SourceId,
  type SourceInfo,
  type SourceTrouble,
  type SsQuota,
} from '../types';

/** Folder names and ids of the mock's systems: a custom quarantine may
 * not sit inside one (Rust resolves them with its system database). */
const SYSTEM_FOLDERS: readonly string[] = [
  ...FIXTURE_SYSTEMS,
  ...ORGANIZE_SYSTEMS,
].flatMap((sys) => [sys.folder, sys.id]);

export const MOCK_STORAGE_KEYS = {
  settings: 'coverly.mock.settings',
  profiles: 'coverly.mock.profiles',
  credentials: 'coverly.mock.credentials',
} as const;

export type MockSettingsApi = Pick<
  Backend,
  | 'getSettings'
  | 'updateSettings'
  | 'resetSettings'
  | 'onSettingsChanged'
  | 'openConfigFolder'
  | 'openLogsFolder'
  | 'openLink'
  | 'openCredit'
  | 'catalogStatus'
  | 'catalogUpdate'
  | 'onCatalogProgress'
  | 'catalogCancel'
  | 'catalogRemove'
  | 'catalogIcon'
  | 'cacheUsage'
  | 'clearCache'
  | 'listProfiles'
  | 'saveProfile'
  | 'deleteProfile'
  | 'previewProfile'
  | 'credentialStatus'
  | 'setCredential'
  | 'clearCredential'
  | 'sourcesInfo'
  | 'testSourceKey'
  | 'integrationsView'
  | 'testIntegration'
  | 'forgetIntegration'
  | 'openIntegrationHelp'
> & {
  /** Folders and links "opened", in order (`folder:logs`, `link:repo`). */
  readonly opened: string[];
  /** The current settings, for the other mock parts. */
  current(): Settings;
  /**
   * Built, every needed credential stored, enabled in the settings, and
   * its key not refused (a fill skips a source whose key is refused).
   */
  isSourceReady(id: SourceId): boolean;
  /** Enabled, but a stored key the fake API refuses (`bad…`). */
  isSourceRefused(id: SourceId): boolean;
  /** Rust's `note_troubles`: a source that answered a run is `connected`,
   * a refused one `error`/`invalid`; a missing key changes nothing
   * (presence already says it). */
  noteTroubles(
    troubles: readonly SourceTrouble[],
    answered: readonly SourceId[],
  ): void;
};

export type MockIntegrationsScenario =
  'empty' | 'saved' | 'error' | 'quota' | 'nodev' | 'dev';
export const MOCK_INTEGRATIONS_SCENARIOS: readonly MockIntegrationsScenario[] =
  ['empty', 'saved', 'error', 'quota', 'nodev', 'dev'];
export type MockKeychainScenario = Exclude<KeychainKind, 'failed'>;
export const MOCK_KEYCHAIN_SCENARIOS: readonly MockKeychainScenario[] = [
  'locked',
  'denied',
  'unavailable',
];
export const MOCK_PLATFORMS = ['macos', 'windows', 'linux'] as const;

function urlSwitch<T extends string>(
  name: string,
  allowed: readonly T[],
): T | null {
  try {
    const v = new URLSearchParams(window.location.search).get(name);
    return (allowed as readonly (string | null)[]).includes(v)
      ? (v as T)
      : null;
  } catch {
    return null; // no window
  }
}

/** `?mockIntegrations=`, or null (the default build). */
export const readMockIntegrations = () =>
  urlSwitch('mockIntegrations', MOCK_INTEGRATIONS_SCENARIOS);
/** `?mockKeychain=`, or null (a keychain that works). */
export const readMockKeychain = () =>
  urlSwitch('mockKeychain', MOCK_KEYCHAIN_SCENARIOS);
/** `?mockPlatform=`, else `macos`. */
export const readMockPlatform = (): string =>
  urlSwitch('mockPlatform', MOCK_PLATFORMS) ?? 'macos';

/** Like Rust's `sources_info`: what a source needs comes from the registry. */
const NEEDS = (id: SourceId): readonly CredentialKey[] =>
  integrationForSource(id)?.fields.map((f) => f.key) ?? [];

/** What the developer-build ScreenScraper test reports. */
const MOCK_SS_QUOTA: SsQuota = {
  requests_today: 1234,
  max_requests_per_day: 20_000,
  max_threads: 1,
};
const MOCK_SS_USED_UP: SsQuota = {
  requests_today: 20_000,
  max_requests_per_day: 20_000,
  max_threads: 1,
};
/** Like Rust's `QUOTA_TTL_SECS`: a used-up quota counts for a day. */
const QUOTA_TTL_SECS = 24 * 60 * 60;

/** What a test or job said, and when (unix seconds). */
interface Seen {
  verdict: KeyCheck;
  at: number;
}

/** TS port of `coverly_core::integrations::state` (D6), for the mock. */
export function mockCardState(i: {
  entry: Integration;
  enabled: boolean;
  built: boolean;
  /** Stored or not per key; a kind = the keychain could not be read. */
  presence: Partial<Record<CredentialKey, boolean>> | KeychainKind;
  health: Seen | undefined;
  quota: { quota: SsQuota; at: number } | null;
  now: number;
}): { state: IntegrationState; reason?: IntegrationReason } {
  if (!i.enabled) return { state: 'off' };
  if (!i.built) return { state: 'unavailable' };
  const presence = i.presence;
  if (typeof presence === 'string')
    return { state: 'error', reason: { kind: 'keychain', detail: presence } };
  const missing = i.entry.fields.find((f) => f.required && !presence[f.key]);
  if (missing)
    return {
      state: 'not-configured',
      reason: { kind: 'missing', detail: missing.input },
    };
  const q = i.quota;
  if (
    q &&
    q.quota.max_requests_per_day > 0 &&
    q.quota.requests_today >= q.quota.max_requests_per_day &&
    i.now - q.at < QUOTA_TTL_SECS
  )
    return { state: 'quota' };
  const v = i.health?.verdict;
  if (v === 'ok') return { state: 'connected' };
  if (v === undefined || v === 'missing') return { state: 'saved' };
  return { state: 'error', reason: { kind: 'check', detail: v } };
}

/** The simulated `titles.<REGION>.<lang>.json` + `ncas.json` download. */
const MOCK_CATALOG_BYTES = 92_400_000;

const CORRUPT = Symbol('corrupt');
/** Larger stored values are treated as corrupt (like Rust's 1 MiB cap). */
const MAX_STORED_CHARS = 1024 * 1024;

function read(storage: Storage | null, key: string): unknown {
  if (!storage) return undefined;
  try {
    const raw = storage.getItem(key);
    if (raw === null) return undefined;
    if (raw.length > MAX_STORED_CHARS) return CORRUPT;
    return JSON.parse(raw) as unknown;
  } catch {
    return CORRUPT;
  }
}

function write(storage: Storage | null, key: string, value: unknown) {
  try {
    storage?.setItem(key, JSON.stringify(value));
  } catch {
    // Storage may be full or blocked: the session keeps working.
  }
}

function remove(storage: Storage | null, key: string) {
  try {
    storage?.removeItem(key);
  } catch {
    // Same as above.
  }
}

const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);
const isOptSize = (v: unknown) =>
  v === null || (typeof v === 'number' && Number.isInteger(v) && v >= 0);

/** The wire shape of a profile file (what serde would accept). */
export function isProfileFile(v: unknown): v is ProfileFile {
  return (
    isObject(v) &&
    typeof v.id === 'string' &&
    typeof v.name === 'string' &&
    typeof v.roms_root === 'string' &&
    Array.isArray(v.art_dirs) &&
    v.art_dirs.every((d) => typeof d === 'string') &&
    typeof v.gamelist === 'boolean' &&
    isObject(v.slots) &&
    Object.values(v.slots).every(
      (s) =>
        isObject(s) &&
        typeof s.template === 'string' &&
        (s.format === 'png' || s.format === 'jpg') &&
        isOptSize(s.max_width) &&
        isOptSize(s.max_height),
    )
  );
}

const mirrors = (p: ProfileFile) =>
  MEDIA.some((m) => p.slots[m]?.template.includes('{rel_dir}') ?? false);

export function createMockSettings(opts: {
  storage: Storage | null;
  builtins: readonly ProfileInfo[];
  /** The external archive tools "found on this machine" (default none,
   * like the organise mock, which leaves every `.rar`). */
  externalTools?: readonly FoundTool[];
  /** How long the simulated Switch catalogue download takes (default
   * 2 s; 0 in tests). */
  catalogMs?: number;
  /** The `?mockIntegrations=` scenario (null = the default build). */
  integrations?: MockIntegrationsScenario | null;
  /** The `?mockKeychain=` failure (null = a keychain that works). */
  keychain?: MockKeychainScenario | null;
  /** «Now» for checks and quotas (unix seconds; default the clock). */
  now?: number;
}): MockSettingsApi {
  const { storage, builtins } = opts;
  const toolsFound = (opts.externalTools ?? []).map((t) => ({ ...t }));
  const foundTools = () => toolsFound.map((t) => ({ ...t }));
  const opened: string[] = [];
  const listeners = new Set<(s: Settings) => void>();
  const builtinIds = builtins.map((b) => b.profile.id);
  const isBuiltin = (id: string) =>
    builtinIds.some((b) => b.toLowerCase() === id.toLowerCase());

  const storedProfiles = read(storage, MOCK_STORAGE_KEYS.profiles);
  let custom: ProfileFile[] = Array.isArray(storedProfiles)
    ? storedProfiles.filter(
        (p): p is ProfileFile =>
          isProfileFile(p) && validateProfile(p, builtinIds).length === 0,
      )
    : [];
  const known = () => [...builtinIds, ...custom.map((p) => p.id)];

  const raw = read(storage, MOCK_STORAGE_KEYS.settings);
  let notice: SettingsNotice | null = null;
  let fixed: string[] = [];
  let settings: Settings;
  if (raw === CORRUPT || (raw !== undefined && !isObject(raw))) {
    settings = cloneSettings(DEFAULT_SETTINGS);
    notice = 'reset-corrupt';
    fixed = ['*'];
    remove(storage, MOCK_STORAGE_KEYS.settings);
  } else if (raw === undefined) {
    settings = cloneSettings(DEFAULT_SETTINGS);
  } else {
    const t = tolerantSettings(raw, known());
    settings = t.settings;
    fixed = t.fixed;
    const version = raw.version;
    if (typeof version === 'number' && version > DEFAULT_SETTINGS.version) {
      notice = 'future-version';
    } else if (t.fixed.length) {
      notice = 'fixed';
    }
  }

  const storedCreds = read(storage, MOCK_STORAGE_KEYS.credentials);
  const creds = Object.fromEntries(
    CREDENTIAL_KEYS.map((k) => [
      k,
      isObject(storedCreds) && storedCreds[k] === true,
    ]),
  ) as Record<CredentialKey, boolean>;
  /** What the fake API says about a stored value (memory only): `bad…`
   * is refused, `offline…` cannot be reached. Never the value itself. */
  const verdicts = new Map<CredentialKey, 'invalid' | 'offline'>();
  const scenario = opts.integrations ?? null;
  const keychain = opts.keychain ?? null;
  const now = () => opts.now ?? Math.floor(Date.now() / 1000);
  /** ScreenScraper is in this build only with developer credentials. */
  const ssBuilt = scenario === 'dev' || scenario === 'quota';
  const built = (id: SourceId) => id !== 'screenscraper' || ssBuilt;
  /** The cards' health and the last ScreenScraper quota (memory only). */
  const health = new Map<string, Seen>();
  let ssQuota: { quota: SsQuota; at: number } | null = null;
  /** Saving or clearing a key: what was said about the old one is gone. */
  const forgetHealthOf = (key: CredentialKey) => {
    const e = WORKING.find((w) => w.fields.some((f) => f.key === key));
    if (!e) return;
    health.delete(e.id);
    if (e.id === 'screenscraper') ssQuota = null;
  };
  const keychainFails = () => {
    if (keychain) throw new BackendError(`keychain-${keychain}`);
  };
  /** Stored and readable (a refusing keychain shows nothing stored). */
  const present = (k: CredentialKey) => keychain === null && creds[k];
  let cacheBytes = 48 * 1024 * 1024;
  /** The Switch catalogue (adjustments D2): downloaded once «Descargar
   * ahora» (or turning it on) ran, in `catalogMs` with progress events
   * like Rust's `catalog-progress`. */
  const catalogMs = opts.catalogMs ?? 2000;
  let catalogRows = false;
  let catalogDownload: CatalogStatus | null = null;
  /** Bumped by a cancel, a removal or turning it off: a running download
   * whose number is no longer current stops and keeps nothing. */
  let catalogRun = 0;
  const catalogListeners = new Set<(s: CatalogStatus) => void>();
  const catalogStop = () => {
    catalogRun += 1;
    catalogDownload = null;
  };
  const catalogStatus = (): CatalogStatus =>
    !settings.catalogs.switch.enabled
      ? { state: 'off' }
      : catalogDownload
        ? { ...catalogDownload }
        : catalogRows
          ? {
              state: 'ready',
              count: 17_342,
              updated: '2026-10-30',
              fallback: false,
            }
          : { state: 'pending' };

  // The scenario's starting point (memory only; the next change saves it).
  const enableSources = (ids: readonly SourceId[]) => {
    settings = cloneSettings(settings);
    settings.sources.order = settings.sources.order.map((e) =>
      ids.includes(e.id) ? { ...e, enabled: true } : e,
    );
  };
  const seed = (keys: readonly CredentialKey[]) => {
    for (const k of CREDENTIAL_KEYS) creds[k] = keys.includes(k);
  };
  const ssKeys: CredentialKey[] = [
    'screenscraper-user',
    'screenscraper-password',
  ];
  switch (scenario) {
    case 'empty':
      seed([]);
      settings = cloneSettings(settings);
      settings.catalogs.switch.enabled = false;
      break;
    case 'saved':
      seed(CREDENTIAL_KEYS);
      enableSources(['steamgriddb']);
      break;
    case 'error':
      creds['steamgriddb-api-key'] = true;
      verdicts.set('steamgriddb-api-key', 'invalid');
      health.set('steamgriddb', { verdict: 'invalid', at: now() });
      enableSources(['steamgriddb']);
      break;
    case 'quota':
      for (const k of ssKeys) creds[k] = true;
      ssQuota = { quota: { ...MOCK_SS_USED_UP }, at: now() };
      enableSources(['screenscraper']);
      break;
    case 'dev':
      for (const k of ssKeys) creds[k] = true;
      enableSources(['screenscraper']);
      break;
    case 'nodev':
    case null:
      break;
  }

  const save = () => write(storage, MOCK_STORAGE_KEYS.settings, settings);
  const emit = () => listeners.forEach((cb) => cb(cloneSettings(settings)));
  const status = (): CredentialStatus => ({
    available: keychain !== 'unavailable',
    set: Object.fromEntries(
      CREDENTIAL_KEYS.map((k) => [k, present(k)]),
    ) as Record<CredentialKey, boolean>,
  });
  const infos = (): ProfileInfo[] => [
    ...builtins.map((b) => structuredClone(b)),
    ...custom.map((p) => ({
      profile: structuredClone(p),
      builtin: false,
      mirror_subdirs: mirrors(p),
    })),
  ];
  const sources = (): SourceInfo[] =>
    KNOWN_SOURCES.map((id) => ({
      id,
      built: built(id),
      needs: [...NEEDS(id)],
      ready: built(id) && NEEDS(id).every(present),
    }));
  /** The fake API's answer to one cheap request with the stored keys. */
  const check = (source: SourceId): IntegrationTestResult => {
    const keys = NEEDS(source);
    if (keys.length === 0)
      throw new BackendError(
        'bad-request',
        `no key to test for \`${String(source)}\``,
      );
    if (!built(source)) return { verdict: 'unavailable' };
    keychainFails();
    if (!keys.every((k) => creds[k])) return { verdict: 'missing' };
    const said = keys.map((k) => verdicts.get(k)).find((v) => v);
    if (said) return { verdict: said };
    if (source !== 'screenscraper') return { verdict: 'ok' };
    const quota = scenario === 'quota' ? MOCK_SS_USED_UP : MOCK_SS_QUOTA;
    return { verdict: 'ok', quota: { ...quota } };
  };
  const enabledOf = (e: Integration) =>
    e.source
      ? settings.sources.order.some((o) => o.id === e.source && o.enabled)
      : e.id === 'switch-catalog'
        ? settings.catalogs.switch.enabled
        : true;
  /** Like Rust's `view_of`, field order and left-out optionals included. */
  const view = (): IntegrationView[] =>
    WORKING.map((e) => {
      const on = enabledOf(e);
      const isBuilt = e.source ? built(e.source) : true;
      const presence: Partial<Record<CredentialKey, boolean>> | KeychainKind =
        e.fields.length > 0 && keychain
          ? keychain
          : Object.fromEntries(e.fields.map((f) => [f.key, present(f.key)]));
      const seen = health.get(e.id);
      const quota = e.id === 'screenscraper' ? ssQuota : null;
      const { state, reason } = mockCardState({
        entry: e,
        // A build that cannot use it cannot turn it on either.
        enabled: on || !isBuilt,
        built: isBuilt,
        presence,
        health: seen,
        quota,
        now: now(),
      });
      return {
        id: e.id,
        enabled: on,
        built: isBuilt,
        fields: e.fields.map((f) => ({ key: f.key, set: present(f.key) })),
        state,
        ...(reason && { reason }),
        ...(seen && { checked_at: seen.at }),
        ...(quota && { quota: { ...quota.quota }, quota_at: quota.at }),
      };
    });
  const working = (id: string): Integration => {
    const e = WORKING.find((w) => w.id === id);
    if (!e)
      throw new BackendError(
        'bad-request',
        `\`${id}\` is not a working integration`,
      );
    return e;
  };
  const credentialKey = (key: string): CredentialKey => {
    const k = CREDENTIAL_KEYS.find((c) => c === key);
    if (!k) throw new BackendError('bad-request', 'unknown credential key');
    return k;
  };
  /** Like Rust's `rename_profile_refs`: the default and the max size follow. */
  const renameRefs = (old: string, next: string | null) => {
    const s = cloneSettings(settings);
    let touched = false;
    if (s.library.default_profile === old) {
      s.library.default_profile = next;
      touched = true;
    }
    const size = s.art.max_size[old];
    if (size) {
      delete s.art.max_size[old];
      if (next !== null) s.art.max_size[next] = size;
      touched = true;
    }
    if (!touched) return;
    settings = s;
    save();
    emit();
  };

  return {
    opened,
    current: () => cloneSettings(settings),
    isSourceReady: (id) =>
      sources().some((i) => i.id === id && i.ready) &&
      settings.sources.order.some((e) => e.id === id && e.enabled) &&
      !NEEDS(id).some((k) => verdicts.get(k) === 'invalid'),
    noteTroubles(troubles, answered) {
      const at = now();
      for (const source of answered) {
        const e = integrationForSource(source);
        if (e) health.set(e.id, { verdict: 'ok', at });
      }
      // The mock's jobs only ever report `auth` (no mock run uses up a
      // quota; `?mockIntegrations=quota` seeds that state instead).
      for (const t of troubles) {
        const e = integrationForSource(t.source);
        if (e && t.kind === 'auth')
          health.set(e.id, { verdict: 'invalid', at });
      }
    },
    isSourceRefused: (id) =>
      settings.sources.order.some((e) => e.id === id && e.enabled) &&
      NEEDS(id).some((k) => present(k) && verdicts.get(k) === 'invalid'),
    getSettings: () =>
      Promise.resolve({
        settings: cloneSettings(settings),
        notice,
        backup: notice ? 'mock-storage' : null,
        fixed: [...fixed],
        external_tools_found: foundTools(),
      }),
    async updateSettings(patch) {
      const unchanged = (errors: ReturnType<typeof validateSettings>) => ({
        settings: cloneSettings(settings),
        errors,
        reopen: false,
      });
      const shape = patchShapeErrors(patch);
      if (shape.length) return unchanged(shape);
      // A `null` group or field is its default again (serde defaults).
      const merged = shapeSettings(mergePatch(settings, patch)).settings;
      const errors = validateSettings(merged, known(), SYSTEM_FOLDERS);
      if (errors.length) return unchanged(errors);
      const next = normaliseSettings(merged);
      const reopen = needsReopen(settings, next);
      // Turning the catalogue off deletes it (it is a cache) and stops a
      // running download, like Rust's `catalog_settings_changed`.
      if (settings.catalogs.switch.enabled && !next.catalogs.switch.enabled) {
        catalogStop();
        catalogRows = false;
      }
      settings = next;
      notice = null;
      fixed = [];
      save();
      emit();
      return { settings: cloneSettings(settings), errors: [], reopen };
    },
    async resetSettings() {
      const recent = settings.library.recent;
      settings = cloneSettings(DEFAULT_SETTINGS);
      settings.library.recent = recent;
      notice = null;
      fixed = [];
      save();
      emit();
      return {
        settings: cloneSettings(settings),
        notice: null,
        backup: null,
        fixed: [],
        external_tools_found: foundTools(),
      };
    },
    onSettingsChanged(cb) {
      listeners.add(cb);
      return Promise.resolve(() => {
        listeners.delete(cb);
      });
    },
    async openConfigFolder() {
      opened.push('folder:config');
    },
    async openLogsFolder() {
      opened.push('folder:logs');
    },
    async openLink(link) {
      opened.push(`link:${link}`);
    },
    async openCredit(id, which) {
      // Like Rust: the URL comes from the credits file, never the webview.
      const c = CREDITS.find((e) => e.id === id);
      const url = which === 'site' ? c?.url : c?.license.url;
      if (!url) throw new BackendError('bad-request', `credit ${id}: no link`);
      opened.push(`credit:${id}:${which}`);
    },
    catalogStatus: () => Promise.resolve(catalogStatus()),
    async catalogUpdate() {
      if (!settings.catalogs.switch.enabled)
        throw new BackendError('bad-request', 'the Switch catalogue is off');
      if (catalogRows) return catalogStatus();
      catalogStop();
      const run = catalogRun;
      const steps = 8;
      for (let i = 1; i <= steps; i += 1) {
        const s: CatalogStatus = {
          state: 'downloading',
          done: Math.round((MOCK_CATALOG_BYTES * (i - 1)) / steps),
          total: MOCK_CATALOG_BYTES,
        };
        catalogDownload = s;
        catalogListeners.forEach((cb) => cb({ ...s }));
        await new Promise<void>((r) => setTimeout(r, catalogMs / steps));
        // Cancelled, removed or turned off meanwhile: nothing is kept.
        if (run !== catalogRun) return catalogStatus();
      }
      catalogDownload = null;
      catalogRows = true;
      return catalogStatus();
    },
    onCatalogProgress(cb) {
      catalogListeners.add(cb);
      return Promise.resolve(() => {
        catalogListeners.delete(cb);
      });
    },
    async catalogCancel() {
      catalogStop();
    },
    async catalogRemove() {
      catalogStop();
      catalogRows = false;
      return catalogStatus();
    },
    async catalogIcon() {
      // The mock has no eShop icons (D8: off unless asked for).
      throw new BackendError('art-missing', 'icon');
    },
    cacheUsage: () => Promise.resolve(cacheBytes),
    async clearCache() {
      const freed = cacheBytes;
      cacheBytes = 0;
      return freed;
    },
    listProfiles: () => Promise.resolve(infos()),
    async saveProfile(profile, replaces) {
      if (!isProfileFile(profile)) {
        throw new BackendError('bad-request', 'not a profile');
      }
      if (replaces !== null && !custom.some((c) => c.id === replaces)) {
        throw new BackendError('bad-request', `no custom profile ${replaces}`);
      }
      const errors = validateProfile(profile, builtinIds);
      const taken = custom.some(
        (c) =>
          c.id.toLowerCase() === profile.id.toLowerCase() && c.id !== replaces,
      );
      if (taken && !errors.some((e) => e.field === 'id')) {
        errors.push({ field: 'id', code: 'duplicate', min: null, max: null });
      }
      if (errors.length) return { profiles: infos(), errors };
      custom = [
        ...custom.filter((c) => c.id !== replaces && c.id !== profile.id),
        structuredClone(profile),
      ].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
      write(storage, MOCK_STORAGE_KEYS.profiles, custom);
      if (replaces !== null && replaces !== profile.id) {
        renameRefs(replaces, profile.id);
      }
      return { profiles: infos(), errors: [] };
    },
    async deleteProfile(id) {
      if (isBuiltin(id)) {
        throw new BackendError(
          'bad-request',
          'built-in profiles are read-only',
        );
      }
      if (!custom.some((c) => c.id === id)) {
        throw new BackendError('unknown-profile', id);
      }
      custom = custom.filter((c) => c.id !== id);
      write(storage, MOCK_STORAGE_KEYS.profiles, custom);
      renameRefs(id, null);
      return infos();
    },
    async previewProfile(profile, example) {
      if (!isProfileFile(profile)) {
        throw new BackendError('bad-request', 'not a profile');
      }
      return {
        paths: previewProfile(profile, example),
        // Like Rust: the id may be a built-in's while duplicating it.
        errors: validateProfile(profile, builtinIds).filter(
          (e) => e.field !== 'id',
        ),
      };
    },
    credentialStatus: () => Promise.resolve(status()),
    async setCredential(key, value) {
      const k = credentialKey(key);
      const v = value.trim();
      if ([...v].length > 512 || /\p{Cc}/u.test(v)) {
        throw new BackendError('bad-request', 'credential value');
      }
      keychainFails();
      creds[k] = v.length > 0;
      if (v.startsWith('bad')) verdicts.set(k, 'invalid');
      else if (v.startsWith('offline')) verdicts.set(k, 'offline');
      else verdicts.delete(k);
      forgetHealthOf(k);
      write(storage, MOCK_STORAGE_KEYS.credentials, creds);
      return status();
    },
    async clearCredential(key) {
      const k = credentialKey(key);
      keychainFails();
      creds[k] = false;
      verdicts.delete(k);
      forgetHealthOf(k);
      write(storage, MOCK_STORAGE_KEYS.credentials, creds);
      return status();
    },
    sourcesInfo: () => Promise.resolve(sources()),
    async testSourceKey(source): Promise<KeyCheck> {
      if (!(KNOWN_SOURCES as readonly string[]).includes(source))
        throw new BackendError(
          'bad-request',
          `no key to test for \`${String(source)}\``,
        );
      return check(source).verdict;
    },
    integrationsView: () => Promise.resolve(view()),
    async testIntegration(id) {
      const e = working(id);
      if (!e.test || !e.source)
        throw new BackendError(
          'bad-request',
          `\`${id}\` has no connection test`,
        );
      const result = check(e.source);
      const at = now();
      health.set(id, { verdict: result.verdict, at });
      if (result.quota) ssQuota = { quota: { ...result.quota }, at };
      return result;
    },
    async forgetIntegration(id) {
      const e = working(id);
      if (e.fields.length === 0)
        throw new BackendError('bad-request', `\`${id}\` has no credentials`);
      keychainFails();
      for (const f of e.fields) {
        creds[f.key] = false;
        verdicts.delete(f.key);
        forgetHealthOf(f.key);
      }
      write(storage, MOCK_STORAGE_KEYS.credentials, creds);
      return view();
    },
    async openIntegrationHelp(id) {
      // Like Rust: the URL comes from the registry, never the webview.
      if (!integration(id)?.help_url)
        throw new BackendError('bad-request', `\`${id}\` has no help page`);
      opened.push(`help:${id}`);
    },
  };
}
