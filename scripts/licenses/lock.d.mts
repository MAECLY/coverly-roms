/** Types for lock.mjs, so TypeScript tests can import it. */
export interface NoticeFile {
  kind: 'rust' | 'js';
  file: string;
  lock: string;
  normalise: (text: string) => string;
}

export const LOCK_PREFIX: string;
export const NOTICE_FILES: readonly NoticeFile[];
export function normaliseCargoLock(text: string): string;
export function normalisePnpmLock(text: string): string;
export function sha256(text: string): string;
export function expectedHash(root: string, entry: NoticeFile): string;
export function declaredHash(text: string): string | null;
export function checkNotices(root: string): string[];
