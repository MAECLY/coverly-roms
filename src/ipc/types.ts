/** Wire types: mirror of src-tauri/src/library/{view,identify}.rs. */

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

export type IdMethod = 'hash' | 'arcade-dat' | 'name' | 'unknown';

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
