/** Backend failures: the Rust `IpcError` wire shape, localised by code. */

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
  'art-unreadable',
  'unknown',
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];

export class BackendError extends Error {
  readonly code: ErrorCode;
  readonly detail: string | undefined;

  constructor(code: ErrorCode, detail?: string) {
    super(detail ? `${code}: ${detail}` : code);
    this.name = 'BackendError';
    this.code = code;
    this.detail = detail;
  }
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
