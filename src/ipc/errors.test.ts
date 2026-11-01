import { describe, expect, it } from 'vitest';

import { translate } from '@/i18n/core';

import { BackendError, ERROR_CODES, errorKey, toBackendError } from './errors';

describe('backend errors', () => {
  it('maps the Rust wire shape', () => {
    const e = toBackendError({ code: 'not-found', detail: '/x' });
    expect(e).toBeInstanceOf(BackendError);
    expect(e.code).toBe('not-found');
    expect(e.detail).toBe('/x');
  });

  it('keeps numeric details as text and unit variants without detail', () => {
    expect(toBackendError({ code: 'unknown-entry', detail: 5 }).detail).toBe(
      '5',
    );
    expect(toBackendError({ code: 'no-library' }).detail).toBeUndefined();
  });

  it('keeps the numbers of a large-transfer refusal as data', () => {
    const detail = {
      bytes: 187_420_000_000,
      items: 3,
      copies: true,
      free: null,
      volume: 'TinyExt',
    };
    const e = toBackendError({ code: 'large-transfer', detail });
    expect(e.code).toBe('large-transfer');
    expect(e.largeTransfer).toEqual(detail);
    // Never "[object Object]" in a message.
    expect(e.detail).toBeUndefined();
    expect(toBackendError(e)).toBe(e);
    expect(
      toBackendError({ code: 'large-transfer', detail: 'x' }).largeTransfer,
    ).toBeUndefined();
    expect(
      new BackendError('large-transfer', undefined, detail).largeTransfer,
    ).toEqual(detail);
  });

  it('turns anything else into an unknown error', () => {
    expect(toBackendError('boom').code).toBe('unknown');
    expect(toBackendError(new Error('bad')).detail).toBe('bad');
    expect(toBackendError({ code: 'made-up' }).code).toBe('unknown');
  });

  it.each(ERROR_CODES)('%s has a message in both languages', (code) => {
    expect(translate('en', errorKey(code))).not.toBe(errorKey(code));
    expect(translate('es', errorKey(code))).not.toBe(errorKey(code));
  });
});
