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
 * `invalid` (only that boolean is remembered, in memory, never the value);
 * otherwise `ok`. ScreenScraper answers `unavailable`; libretro (no key)
 * is a `bad-request`.
 */
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
import {
  CREDENTIAL_KEYS,
  MEDIA,
  type CredentialKey,
  type CredentialStatus,
  type FoundTool,
  type KeyCheck,
  type ProfileFile,
  type ProfileInfo,
  type Settings,
  type SettingsNotice,
  type SourceId,
  type SourceInfo,
} from '../types';

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
};

/** What the mock build carries (ScreenScraper needs dev credentials). */
const BUILT: Record<SourceId, boolean> = {
  libretro: true,
  steamgriddb: true,
  screenscraper: false,
};
const NEEDS: Record<SourceId, readonly CredentialKey[]> = {
  libretro: [],
  steamgriddb: ['steamgriddb-api-key'],
  screenscraper: [],
};

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
  /** Keys whose stored value would be refused by the source (memory only). */
  const rejected = new Set<CredentialKey>();
  let cacheBytes = 48 * 1024 * 1024;

  const save = () => write(storage, MOCK_STORAGE_KEYS.settings, settings);
  const emit = () => listeners.forEach((cb) => cb(cloneSettings(settings)));
  const status = (): CredentialStatus => ({
    available: true,
    set: { ...creds },
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
      built: BUILT[id],
      needs: [...NEEDS[id]],
      ready: BUILT[id] && NEEDS[id].every((k) => creds[k]),
    }));
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
      !NEEDS[id].some((k) => rejected.has(k)),
    isSourceRefused: (id) =>
      settings.sources.order.some((e) => e.id === id && e.enabled) &&
      NEEDS[id].some((k) => creds[k] && rejected.has(k)),
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
      const errors = validateSettings(merged, known());
      if (errors.length) return unchanged(errors);
      const next = normaliseSettings(merged);
      const reopen = needsReopen(settings, next);
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
      creds[k] = v.length > 0;
      if (v.startsWith('bad')) rejected.add(k);
      else rejected.delete(k);
      write(storage, MOCK_STORAGE_KEYS.credentials, creds);
      return status();
    },
    async clearCredential(key) {
      const k = credentialKey(key);
      creds[k] = false;
      rejected.delete(k);
      write(storage, MOCK_STORAGE_KEYS.credentials, creds);
      return status();
    },
    sourcesInfo: () => Promise.resolve(sources()),
    async testSourceKey(source): Promise<KeyCheck> {
      switch (source) {
        case 'steamgriddb': {
          const k: CredentialKey = 'steamgriddb-api-key';
          if (!creds[k]) return 'missing';
          return rejected.has(k) ? 'invalid' : 'ok';
        }
        case 'screenscraper':
          return 'unavailable';
        default:
          throw new BackendError(
            'bad-request',
            `no key to test for \`${String(source)}\``,
          );
      }
    },
  };
}
