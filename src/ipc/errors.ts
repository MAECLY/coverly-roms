/** Backend failures: the Rust `IpcError` wire shape, localised by code. */

import type { LargeTransferDetail } from './types';

export const ERROR_CODES = [
  'not-found',
  'not-a-folder',
  'unreadable',
  'no-profile',
  'unknown-profile',
  'no-library',
  'stale-library',
  'superseded',
  'unknown-entry',
  'art-missing',
  'outside-library',
  'art-too-large',
  'io',
  'settings',
  'job-running',
  'no-job',
  'stale-job',
  'bad-request',
  'nothing-to-undo',
  'network',
  'bad-image',
  'write-denied',
  'disk-full',
  'name-invalid',
  'volume-gone',
  'keychain-unavailable',
  'keychain-locked',
  'keychain-failed',
  'update-offline',
  'update-signature',
  'update-unavailable',
  'update-no-key',
  'update-failed',
  'update-not-ready',
  'art-unreadable',
  'cancelled',
  'no-plan',
  'later-jobs',
  'no-space',
  'no-dedupe-plan',
  'keeper-required',
  'quarantine-in-system-folder',
  'large-transfer',
  'unknown',
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];

export class BackendError extends Error {
  readonly code: ErrorCode;
  readonly detail: string | undefined;
  /** `large-transfer` only: what the decided plan would write (spec D23). */
  readonly largeTransfer: LargeTransferDetail | undefined;

  constructor(
    code: ErrorCode,
    detail?: string,
    largeTransfer?: LargeTransferDetail,
  ) {
    super(detail ? `${code}: ${detail}` : code);
    this.name = 'BackendError';
    this.code = code;
    this.detail = detail;
    this.largeTransfer = largeTransfer;
  }
}

function isLargeTransfer(value: unknown): value is LargeTransferDetail {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.bytes === 'number' &&
    typeof v.items === 'number' &&
    typeof v.copies === 'boolean' &&
    (v.free === null || typeof v.free === 'number') &&
    typeof v.volume === 'string'
  );
}

function isErrorCode(value: unknown): value is ErrorCode {
  return (
    typeof value === 'string' &&
    (ERROR_CODES as readonly string[]).includes(value)
  );
}

export function toBackendError(e: unknown): BackendError {
  if (e instanceof BackendError) return e;
  if (typeof e === 'object' && e !== null && 'code' in e) {
    const { code, detail } = e as { code: unknown; detail?: unknown };
    if (code === 'large-transfer') {
      // A struct variant: its numbers stay data for the dialog.
      return new BackendError(
        code,
        undefined,
        isLargeTransfer(detail)
          ? {
              bytes: detail.bytes,
              items: detail.items,
              copies: detail.copies,
              free: detail.free,
              volume: detail.volume,
            }
          : undefined,
      );
    }
    if (isErrorCode(code)) {
      return new BackendError(
        code,
        detail === undefined || detail === null ? undefined : String(detail),
      );
    }
  }
  return new BackendError(
    'unknown',
    e instanceof Error ? e.message : String(e),
  );
}

/** The i18n key of a backend error. */
export function errorKey(code: ErrorCode): string {
  return `errors.${code}`;
}
