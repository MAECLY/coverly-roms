import { describe, expect, it } from 'vitest';

import sample from './contract.sample.json';
import type {
  EntryView,
  IdentityView,
  LibraryView,
  ProfileView,
  RecentLibrary,
  ScanProgress,
  SystemView,
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
});
